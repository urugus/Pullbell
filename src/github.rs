use crate::model::{PrKind, PullRequestItem, merge_pr_items};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;

const API_VERSION: &str = "2022-11-28";
const USER_AGENT: &str = concat!("pullbell/", env!("CARGO_PKG_VERSION"));

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
        }

        let encoded_query = urlencoding::encode(query);
        let url = format!(
            "https://api.github.com/search/issues?q={encoded_query}&sort=updated&order=desc&per_page=50"
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

        Ok(response
            .items
            .into_iter()
            .map(|item| {
                let repo = repo_name_from_repository_url(&item.repository_url);
                PullRequestItem {
                    id: pr_id(&repo, item.number),
                    repo,
                    title: item.title,
                    url: item.html_url,
                    number: item.number,
                    updated_at: item.updated_at,
                    kind: kind.clone(),
                }
            })
            .collect())
    }

    async fn pull_request_notifications(&self) -> Result<Vec<PullRequestItem>> {
        #[derive(Deserialize)]
        struct Notification {
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
            #[serde(rename = "type")]
            kind: String,
        }

        let notifications = self
            .get("https://api.github.com/notifications?per_page=100")
            .send()
            .await
            .context("loading GitHub notifications")?
            .error_for_status()
            .context("GitHub rejected notifications request")?
            .json::<Vec<Notification>>()
            .await
            .context("decoding GitHub notifications response")?;

        Ok(notifications
            .into_iter()
            .filter(|notification| notification.subject.kind == "PullRequest")
            .filter_map(|notification| {
                let api_url = notification.subject.url?;
                let number = api_url.rsplit('/').next()?.parse().ok()?;
                let url = api_url
                    .replace("https://api.github.com/repos/", "https://github.com/")
                    .replace("/pulls/", "/pull/");

                Some(PullRequestItem {
                    id: pr_id(&notification.repository.full_name, number),
                    repo: notification.repository.full_name,
                    title: notification.subject.title,
                    url,
                    number,
                    updated_at: notification.updated_at,
                    kind: PrKind::Notification,
                })
            })
            .collect())
    }

    async fn teams(&self) -> Result<Vec<Team>> {
        self.get("https://api.github.com/user/teams?per_page=100")
            .send()
            .await
            .context("loading GitHub teams")?
            .error_for_status()
            .context("GitHub rejected teams request")?
            .json::<Vec<Team>>()
            .await
            .context("decoding GitHub teams response")
    }

    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.http
            .get(url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("User-Agent", USER_AGENT)
    }
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
}
