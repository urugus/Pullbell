use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
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
            Self::Authored => 1,
            Self::Notification => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestItem {
    pub id: String,
    pub repo: String,
    pub title: String,
    pub url: String,
    pub number: u64,
    pub updated_at: Option<DateTime<Utc>>,
    pub kind: PrKind,
}

impl PullRequestItem {
    pub fn display_title(&self) -> String {
        format!("{} #{}: {}", self.repo, self.number, self.title)
    }

    pub fn is_todo(&self) -> bool {
        self.kind.is_todo()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub latest_version: String,
    pub release_url: String,
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
        }
    }

    #[test]
    fn merge_prefers_the_most_actionable_kind_for_duplicates() {
        let merged = merge_pr_items(vec![
            item("1", PrKind::Notification, 10),
            item("1", PrKind::ReviewRequested, 20),
            item("2", PrKind::Authored, 30),
        ]);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, "1");
        assert_eq!(merged[0].kind, PrKind::ReviewRequested);
        assert_eq!(
            merged[0].updated_at,
            Some(Utc.timestamp_opt(20, 0).unwrap())
        );
        assert_eq!(merged[1].kind, PrKind::Authored);
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
        assert_eq!(ids, vec!["2", "4", "1", "3"]);
    }

    #[test]
    fn identifies_items_that_need_action() {
        assert!(item("1", PrKind::ReviewRequested, 10).is_todo());
        assert!(item("2", PrKind::Notification, 10).is_todo());
        assert!(!item("3", PrKind::Authored, 10).is_todo());
    }
}
