use crate::model::{PrKind, PullRequestItem, merge_pr_items};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};

const API_VERSION: &str = "2022-11-28";
const USER_AGENT: &str = concat!("pullbell/", env!("CARGO_PKG_VERSION"));
const MAX_NOTIFICATION_PREVIEWS: usize = 12;
const SEARCH_PAGE_SIZE: usize = 50;
const NOTIFICATIONS_PAGE_SIZE: usize = 100;
const TEAMS_PAGE_SIZE: usize = 100;
const MAX_SEARCH_PAGES: u32 = 20;
const MAX_LIST_PAGES: u32 = 10;

#[derive(Clone)]
pub struct GitHubClient {
    http: Client,
    token: String,
}

#[derive(Debug, Clone)]
pub struct Viewer {
    pub login: String,
}

impl GitHubClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            token: token.into(),
        }
    }

    pub async fn viewer(&self) -> Result<Viewer> {
        #[derive(Deserialize)]
        struct Response {
            login: String,
        }

        let response = self
            .get("https://api.github.com/user")
            .send()
            .await
            .context("loading GitHub viewer")?
            .error_for_status()
            .context("GitHub rejected the viewer request")?
            .json::<Response>()
            .await
            .context("decoding GitHub viewer response")?;

        Ok(Viewer {
            login: response.login,
        })
    }

    pub async fn pull_requests_for(&self, viewer_login: &str) -> Result<Vec<PullRequestItem>> {
        let mut items = Vec::new();

        items.extend(
            self.search_pull_requests(
                &format!(
                    "is:pr is:open archived:false review-requested:{}",
                    viewer_login
                ),
                PrKind::ReviewRequested,
            )
            .await
            .context("loading review requests")?,
        );

        for team in self
            .teams()
            .await
            .context("loading teams for review requests")?
        {
            let team_name = format!("{}/{}", team.organization.login, team.slug);
            let query = format!("is:pr is:open archived:false team-review-requested:{team_name}");
            items.extend(
                self.search_pull_requests(&query, PrKind::ReviewRequested)
                    .await
                    .with_context(|| format!("loading team review requests for {team_name}"))?,
            );
        }

        items.extend(
            self.search_pull_requests(
                &format!("is:pr is:open archived:false author:{}", viewer_login),
                PrKind::Authored,
            )
            .await
            .context("loading authored pull requests")?,
        );

        items.extend(
            self.pull_request_notifications()
                .await
                .context("loading pull request notifications")?,
        );

        Ok(merge_pr_items(items))
    }

    async fn search_pull_requests(
        &self,
        query: &str,
        kind: PrKind,
    ) -> Result<Vec<PullRequestItem>> {
        #[derive(Deserialize)]
        struct SearchResponse {
            items: Vec<SearchIssue>,
        }

        #[derive(Deserialize)]
        struct SearchIssue {
            number: u64,
            title: String,
            html_url: String,
            repository_url: String,
            updated_at: Option<DateTime<Utc>>,
            body: Option<String>,
            user: Option<ApiUser>,
        }

        #[derive(Deserialize)]
        struct ApiUser {
            login: String,
        }

        let mut pull_requests = Vec::new();
        let encoded_query = urlencoding::encode(query);

        for page in 1..=MAX_SEARCH_PAGES {
            let url = format!(
                "https://api.github.com/search/issues?q={encoded_query}&sort=updated&order=desc&per_page={SEARCH_PAGE_SIZE}&page={page}"
            );
            let response = self
                .get(&url)
                .send()
                .await
                .with_context(|| format!("searching GitHub pull requests: {query}"))?
                .error_for_status()
                .with_context(|| format!("GitHub rejected search query: {query}"))?
                .json::<SearchResponse>()
                .await
                .context("decoding GitHub search response")?;

            let item_count = response.items.len();
            pull_requests.extend(response.items.into_iter().map(|item| {
                let repo = repo_name_from_repository_url(&item.repository_url);
                PullRequestItem {
                    id: pr_id(&repo, item.number),
                    repo,
                    title: item.title,
                    url: item.html_url,
                    number: item.number,
                    updated_at: item.updated_at,
                    kind: kind.clone(),
                    notification_thread_id: None,
                    author: item.user.map(|user| user.login),
                    reason: Some(search_reason(&kind).to_string()),
                    preview: item.body.and_then(clean_preview),
                }
            }));

            if item_count < SEARCH_PAGE_SIZE {
                break;
            }
        }

        Ok(pull_requests)
    }

    async fn pull_request_notifications(&self) -> Result<Vec<PullRequestItem>> {
        #[derive(Deserialize)]
        struct Notification {
            id: String,
            reason: Option<String>,
            repository: Repository,
            subject: Subject,
            updated_at: Option<DateTime<Utc>>,
        }

        #[derive(Deserialize)]
        struct Repository {
            full_name: String,
        }

        #[derive(Deserialize)]
        struct Subject {
            title: String,
            url: Option<String>,
            latest_comment_url: Option<String>,
            #[serde(rename = "type")]
            kind: String,
        }

        let mut notifications = Vec::new();
        for page in 1..=MAX_LIST_PAGES {
            let url = format!(
                "https://api.github.com/notifications?per_page={NOTIFICATIONS_PAGE_SIZE}&page={page}"
            );
            let page_notifications = self
                .get(&url)
                .send()
                .await
                .context("loading GitHub notifications")?
                .error_for_status()
                .context("GitHub rejected notifications request")?
                .json::<Vec<Notification>>()
                .await
                .context("decoding GitHub notifications response")?;
            let notification_count = page_notifications.len();
            notifications.extend(page_notifications);

            if notification_count < NOTIFICATIONS_PAGE_SIZE {
                break;
            }
        }

        let mut items = Vec::new();
        for (index, notification) in notifications
            .into_iter()
            .filter(|notification| notification.subject.kind == "PullRequest")
            .enumerate()
        {
            let Some(api_url) = notification.subject.url.as_deref() else {
                continue;
            };
            let Some(number) = api_url
                .rsplit('/')
                .next()
                .and_then(|value| value.parse().ok())
            else {
                continue;
            };
            let details = if index < MAX_NOTIFICATION_PREVIEWS {
                self.pull_request_preview(
                    api_url,
                    notification.subject.latest_comment_url.as_deref(),
                )
                .await
                .unwrap_or_default()
            } else {
                PreviewDetails::default()
            };
            let url = details.html_url.unwrap_or_else(|| {
                api_url
                    .replace("https://api.github.com/repos/", "https://github.com/")
                    .replace("/pulls/", "/pull/")
            });

            items.push(PullRequestItem {
                id: pr_id(&notification.repository.full_name, number),
                repo: notification.repository.full_name,
                title: notification.subject.title,
                url,
                number,
                updated_at: notification.updated_at,
                kind: PrKind::Notification,
                notification_thread_id: Some(notification.id),
                author: details.author,
                reason: notification.reason,
                preview: details.preview,
            });
        }

        Ok(items)
    }

    async fn pull_request_preview(
        &self,
        api_url: &str,
        latest_comment_url: Option<&str>,
    ) -> Result<PreviewDetails> {
        #[derive(Deserialize)]
        struct PullRequestResponse {
            html_url: Option<String>,
            body: Option<String>,
            user: Option<ApiUser>,
        }

        #[derive(Deserialize)]
        struct CommentResponse {
            body: Option<String>,
            user: Option<ApiUser>,
        }

        #[derive(Deserialize)]
        struct ApiUser {
            login: String,
        }

        let pull_request = self
            .get(api_url)
            .send()
            .await
            .with_context(|| format!("loading pull request preview: {api_url}"))?
            .error_for_status()
            .with_context(|| format!("GitHub rejected pull request preview: {api_url}"))?
            .json::<PullRequestResponse>()
            .await
            .context("decoding pull request preview response")?;

        let html_url = pull_request.html_url;
        let author = pull_request.user.map(|user| user.login);
        let body_preview = pull_request.body.and_then(clean_preview);

        let comment_response = match latest_comment_url {
            Some(comment_url) => self
                .get(comment_url)
                .send()
                .await
                .ok()
                .and_then(|response| response.error_for_status().ok()),
            None => None,
        };
        let comment = match comment_response {
            Some(response) => response.json::<CommentResponse>().await.ok(),
            None => None,
        };

        if let Some((preview, comment_author)) = comment.and_then(|comment| {
            comment
                .body
                .and_then(clean_preview)
                .map(|preview| (preview, comment.user))
        }) {
            return Ok(PreviewDetails {
                html_url,
                author: comment_author
                    .map(|user| user.login)
                    .or_else(|| author.clone()),
                preview: Some(preview),
            });
        }

        Ok(PreviewDetails {
            html_url,
            author,
            preview: body_preview,
        })
    }

    pub async fn mark_notification_thread_done(&self, thread_id: &str) -> Result<()> {
        let url = notification_thread_url(thread_id);
        self.delete(&url)
            .send()
            .await
            .with_context(|| format!("marking GitHub notification thread {thread_id} as done"))?
            .error_for_status()
            .with_context(|| {
                format!("GitHub rejected marking notification thread {thread_id} as done")
            })?;

        Ok(())
    }

    pub async fn mute_notification_thread(&self, thread_id: &str) -> Result<()> {
        #[derive(Serialize)]
        struct SubscriptionRequest {
            ignored: bool,
        }

        let url = format!("{}/subscription", notification_thread_url(thread_id));
        self.put(&url)
            .json(&SubscriptionRequest { ignored: true })
            .send()
            .await
            .with_context(|| format!("muting GitHub notification thread {thread_id}"))?
            .error_for_status()
            .with_context(|| format!("GitHub rejected muting notification thread {thread_id}"))?;

        Ok(())
    }

    async fn teams(&self) -> Result<Vec<Team>> {
        let mut teams = Vec::new();
        for page in 1..=MAX_LIST_PAGES {
            let url =
                format!("https://api.github.com/user/teams?per_page={TEAMS_PAGE_SIZE}&page={page}");
            let page_teams = self
                .get(&url)
                .send()
                .await
                .context("loading GitHub teams")?
                .error_for_status()
                .context("GitHub rejected teams request")?
                .json::<Vec<Team>>()
                .await
                .context("decoding GitHub teams response")?;
            let team_count = page_teams.len();
            teams.extend(page_teams);

            if team_count < TEAMS_PAGE_SIZE {
                break;
            }
        }

        Ok(teams)
    }

    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.http
            .get(url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("User-Agent", USER_AGENT)
    }

    fn delete(&self, url: &str) -> reqwest::RequestBuilder {
        self.http
            .delete(url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("User-Agent", USER_AGENT)
    }

    fn put(&self, url: &str) -> reqwest::RequestBuilder {
        self.http
            .put(url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("User-Agent", USER_AGENT)
    }
}

#[derive(Debug, Default)]
struct PreviewDetails {
    html_url: Option<String>,
    author: Option<String>,
    preview: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Team {
    slug: String,
    organization: TeamOrganization,
}

#[derive(Debug, Deserialize)]
struct TeamOrganization {
    login: String,
}

fn repo_name_from_repository_url(url: &str) -> String {
    url.strip_prefix("https://api.github.com/repos/")
        .unwrap_or(url)
        .to_string()
}

fn pr_id(repo: &str, number: u64) -> String {
    format!("{repo}#{number}")
}

fn notification_thread_url(thread_id: &str) -> String {
    format!("https://api.github.com/notifications/threads/{thread_id}")
}

fn search_reason(kind: &PrKind) -> &'static str {
    match kind {
        PrKind::ReviewRequested => "review_requested",
        PrKind::Authored => "author",
        PrKind::Notification => "notification",
    }
}

fn clean_preview(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.chars().take(1_200).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_repo_name_from_api_url() {
        assert_eq!(
            repo_name_from_repository_url("https://api.github.com/repos/tokium/example"),
            "tokium/example"
        );
    }

    #[test]
    fn builds_stable_pr_id() {
        assert_eq!(pr_id("tokium/example", 123), "tokium/example#123");
    }

    #[test]
    fn builds_notification_thread_url() {
        assert_eq!(
            notification_thread_url("123"),
            "https://api.github.com/notifications/threads/123"
        );
    }
}
