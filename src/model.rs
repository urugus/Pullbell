use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrKind {
    ReviewRequested,
    Authored,
    Notification,
}

impl PrKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ReviewRequested => "Needs your review",
            Self::Authored => "Your open PRs",
            Self::Notification => "Unread PR notifications",
        }
    }

    pub fn is_todo(&self) -> bool {
        matches!(self, Self::ReviewRequested | Self::Notification)
    }

    fn priority(&self) -> u8 {
        match self {
            Self::ReviewRequested => 0,
            Self::Notification => 1,
            Self::Authored => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestItem {
    pub id: String,
    pub repo: String,
    pub title: String,
    pub url: String,
    pub number: u64,
    pub updated_at: Option<DateTime<Utc>>,
    pub kind: PrKind,
    pub notification_thread_id: Option<String>,
    pub author: Option<String>,
    pub reason: Option<String>,
    pub preview: Option<String>,
    pub locally_done: bool,
}

impl PullRequestItem {
    pub fn display_title(&self) -> String {
        format!("{} #{}: {}", self.repo, self.number, self.title)
    }

    pub fn is_todo(&self) -> bool {
        self.kind.is_todo() && !self.locally_done
    }

    pub fn local_done_snapshot(&self) -> Self {
        let mut snapshot = self.clone();
        snapshot.locally_done = true;
        snapshot.preview = None;
        snapshot
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalDonePr {
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub item: Option<PullRequestItem>,
}

pub type LocalDonePrs = BTreeMap<String, LocalDonePr>;

pub fn apply_local_done_prs(items: &mut Vec<PullRequestItem>, local_done_prs: &mut LocalDonePrs) {
    let mut stale_ids = Vec::new();

    for item in items.iter_mut() {
        let Some(done) = local_done_prs.get_mut(&item.id) else {
            item.locally_done = false;
            continue;
        };

        if local_done_still_applies(done.updated_at, item.updated_at) {
            item.locally_done = true;
            done.item = Some(item.local_done_snapshot());
        } else {
            item.locally_done = false;
            stale_ids.push(item.id.clone());
        }
    }

    for id in stale_ids {
        local_done_prs.remove(&id);
    }

    for (id, done) in local_done_prs.iter() {
        if items.iter().any(|item| &item.id == id) {
            continue;
        }

        if let Some(item) = done.item.clone() {
            let item = item.local_done_snapshot();
            items.push(item);
        }
    }
}

fn local_done_still_applies(
    done_updated_at: Option<DateTime<Utc>>,
    current_updated_at: Option<DateTime<Utc>>,
) -> bool {
    match (done_updated_at, current_updated_at) {
        (Some(done_updated_at), Some(current_updated_at)) => done_updated_at >= current_updated_at,
        _ => true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub latest_version: String,
    pub release_url: String,
    pub download_url: Option<String>,
    pub download_name: Option<String>,
    pub checksum_url: Option<String>,
}

pub fn merge_pr_items(items: Vec<PullRequestItem>) -> Vec<PullRequestItem> {
    let mut by_id: BTreeMap<String, PullRequestItem> = BTreeMap::new();

    for item in items {
        by_id
            .entry(item.id.clone())
            .and_modify(|existing| {
                if item.kind.priority() < existing.kind.priority() {
                    existing.kind = item.kind.clone();
                }
                if item.updated_at > existing.updated_at {
                    existing.updated_at = item.updated_at;
                }
                if existing.notification_thread_id.is_none() {
                    existing.notification_thread_id = item.notification_thread_id.clone();
                }
                if existing.author.is_none() {
                    existing.author = item.author.clone();
                }
                if existing.reason.is_none() {
                    existing.reason = item.reason.clone();
                }
                if existing.preview.is_none() {
                    existing.preview = item.preview.clone();
                }
            })
            .or_insert(item);
    }

    let mut merged: Vec<_> = by_id.into_values().collect();
    merged.sort_by(|left, right| {
        left.kind
            .priority()
            .cmp(&right.kind.priority())
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.repo.cmp(&right.repo))
            .then_with(|| left.number.cmp(&right.number))
    });
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;

    fn item(id: &str, kind: PrKind, updated_at: i64) -> PullRequestItem {
        PullRequestItem {
            id: id.to_string(),
            repo: "org/repo".to_string(),
            title: format!("PR {id}"),
            url: format!("https://github.com/org/repo/pull/{id}"),
            number: id.parse().unwrap_or(1),
            updated_at: Some(Utc.timestamp_opt(updated_at, 0).unwrap()),
            kind,
            notification_thread_id: None,
            author: None,
            reason: None,
            preview: None,
            locally_done: false,
        }
    }

    fn notification_item(id: &str, thread_id: &str, updated_at: i64) -> PullRequestItem {
        PullRequestItem {
            notification_thread_id: Some(thread_id.to_string()),
            ..item(id, PrKind::Notification, updated_at)
        }
    }

    #[test]
    fn merge_prefers_the_most_actionable_kind_for_duplicates() {
        let merged = merge_pr_items(vec![
            item("1", PrKind::Notification, 10),
            item("1", PrKind::ReviewRequested, 20),
            item("2", PrKind::Authored, 30),
            item("2", PrKind::Notification, 40),
            item("3", PrKind::Authored, 50),
        ]);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].id, "1");
        assert_eq!(merged[0].kind, PrKind::ReviewRequested);
        assert_eq!(
            merged[0].updated_at,
            Some(Utc.timestamp_opt(20, 0).unwrap())
        );
        assert_eq!(merged[1].id, "2");
        assert_eq!(merged[1].kind, PrKind::Notification);
        assert_eq!(
            merged[1].updated_at,
            Some(Utc.timestamp_opt(40, 0).unwrap())
        );
        assert_eq!(merged[2].kind, PrKind::Authored);
    }

    #[test]
    fn merge_prefers_unread_notifications_over_authored_prs() {
        let merged = merge_pr_items(vec![
            item("2", PrKind::Authored, 30),
            notification_item("2", "thread-2", 40),
        ]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, "2");
        assert_eq!(merged[0].kind, PrKind::Notification);
        assert_eq!(
            merged[0].notification_thread_id.as_deref(),
            Some("thread-2")
        );
        assert_eq!(
            merged[0].updated_at,
            Some(Utc.timestamp_opt(40, 0).unwrap())
        );
    }

    #[test]
    fn merge_preserves_notification_thread_id_when_review_request_is_more_actionable() {
        let merged = merge_pr_items(vec![
            notification_item("1", "thread-1", 10),
            item("1", PrKind::ReviewRequested, 20),
        ]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].kind, PrKind::ReviewRequested);
        assert_eq!(
            merged[0].notification_thread_id.as_deref(),
            Some("thread-1")
        );
    }

    #[test]
    fn merge_preserves_preview_metadata_from_duplicates() {
        let merged = merge_pr_items(vec![
            PullRequestItem {
                author: Some("octo".to_string()),
                reason: Some("comment".to_string()),
                preview: Some("Looks good".to_string()),
                ..notification_item("1", "thread-1", 10)
            },
            item("1", PrKind::ReviewRequested, 20),
        ]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].author.as_deref(), Some("octo"));
        assert_eq!(merged[0].reason.as_deref(), Some("comment"));
        assert_eq!(merged[0].preview.as_deref(), Some("Looks good"));
    }

    #[test]
    fn local_done_prs_keep_matching_items_done() {
        let mut items = vec![item("1", PrKind::ReviewRequested, 20)];
        let mut done = LocalDonePrs::from([(
            "1".to_string(),
            LocalDonePr {
                updated_at: Some(Utc.timestamp_opt(20, 0).unwrap()),
                item: None,
            },
        )]);

        apply_local_done_prs(&mut items, &mut done);

        assert!(!items[0].is_todo());
        assert!(items[0].locally_done);
        assert!(done.contains_key("1"));
    }

    #[test]
    fn local_done_prs_expire_when_items_are_newer() {
        let mut items = vec![item("1", PrKind::ReviewRequested, 30)];
        let mut done = LocalDonePrs::from([(
            "1".to_string(),
            LocalDonePr {
                updated_at: Some(Utc.timestamp_opt(20, 0).unwrap()),
                item: None,
            },
        )]);

        apply_local_done_prs(&mut items, &mut done);

        assert!(items[0].is_todo());
        assert!(!items[0].locally_done);
        assert!(!done.contains_key("1"));
    }

    #[test]
    fn local_done_prs_without_timestamps_keep_items_done() {
        let mut items = vec![PullRequestItem {
            updated_at: None,
            ..item("1", PrKind::ReviewRequested, 20)
        }];
        let mut done = LocalDonePrs::from([(
            "1".to_string(),
            LocalDonePr {
                updated_at: None,
                item: None,
            },
        )]);

        apply_local_done_prs(&mut items, &mut done);

        assert!(!items[0].is_todo());
        assert!(items[0].locally_done);
    }

    #[test]
    fn local_done_snapshot_drops_preview_text() {
        let snapshot = PullRequestItem {
            preview: Some("private preview".to_string()),
            ..item("1", PrKind::ReviewRequested, 20)
        }
        .local_done_snapshot();

        assert!(snapshot.locally_done);
        assert_eq!(snapshot.preview, None);
    }

    #[test]
    fn local_done_prs_append_missing_snapshot_items() {
        let snapshot = PullRequestItem {
            locally_done: true,
            preview: Some("private preview".to_string()),
            ..item("1", PrKind::Notification, 20)
        };
        let mut items = vec![item("2", PrKind::Authored, 30)];
        let mut done = LocalDonePrs::from([(
            "1".to_string(),
            LocalDonePr {
                updated_at: Some(Utc.timestamp_opt(20, 0).unwrap()),
                item: Some(snapshot),
            },
        )]);

        apply_local_done_prs(&mut items, &mut done);

        assert_eq!(items.len(), 2);
        assert_eq!(items[1].id, "1");
        assert!(items[1].locally_done);
        assert_eq!(items[1].preview, None);
        assert!(!items[1].is_todo());
    }

    #[test]
    fn merge_sorts_by_actionability_then_recency() {
        let merged = merge_pr_items(vec![
            item("3", PrKind::Notification, 300),
            item("1", PrKind::Authored, 100),
            item("2", PrKind::ReviewRequested, 10),
            item("4", PrKind::Authored, 400),
        ]);

        let ids: Vec<_> = merged.into_iter().map(|item| item.id).collect();
        assert_eq!(ids, vec!["2", "3", "4", "1"]);
    }

    #[test]
    fn identifies_items_that_need_action() {
        assert!(item("1", PrKind::ReviewRequested, 10).is_todo());
        assert!(item("2", PrKind::Notification, 10).is_todo());
        assert!(!item("3", PrKind::Authored, 10).is_todo());
    }
}
