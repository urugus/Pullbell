use crate::model::{AvailableUpdate, LocalDonePrs, PullRequestItem};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub signed_in_as: Option<String>,
    pub token_loaded: bool,
    pub is_refreshing: bool,
    pub is_checking_updates: bool,
    pub last_error: Option<String>,
    pub last_status: Option<String>,
    pub update_status: Option<String>,
    pub last_refreshed_at: Option<DateTime<Utc>>,
    pub last_update_checked_at: Option<DateTime<Utc>>,
    pub available_update: Option<AvailableUpdate>,
    pub pending_auth: Option<PendingAuth>,
    pub pull_requests: Vec<PullRequestItem>,
    pub local_done_prs: LocalDonePrs,
}

#[derive(Debug, Clone)]
pub struct PendingAuth {
    pub user_code: String,
    pub verification_uri: String,
}

impl AppState {
    pub fn tray_title(&self) -> String {
        let (count, _) = self.todo_done_counts();
        if count == 0 {
            "PR".to_string()
        } else {
            format!("PR {count}")
        }
    }

    pub fn todo_count(&self) -> usize {
        self.todo_done_counts().0
    }

    pub fn done_count(&self) -> usize {
        self.todo_done_counts().1
    }

    pub fn todo_done_counts(&self) -> (usize, usize) {
        self.pull_requests
            .iter()
            .fold((0, 0), |(todo, done), item| {
                if item.is_todo() {
                    (todo + 1, done)
                } else {
                    (todo, done + 1)
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PrKind, PullRequestItem};

    fn item(id: &str, kind: PrKind) -> PullRequestItem {
        PullRequestItem {
            id: id.to_string(),
            repo: "org/repo".to_string(),
            title: format!("PR {id}"),
            url: format!("https://github.com/org/repo/pull/{id}"),
            number: id.parse().unwrap_or(1),
            updated_at: None,
            kind,
            notification_thread_id: None,
            author: None,
            reason: None,
            preview: None,
            locally_done: false,
        }
    }

    #[test]
    fn tray_title_counts_todo_items_only() {
        let state = AppState {
            pull_requests: vec![
                item("1", PrKind::ReviewRequested),
                item("2", PrKind::Authored),
                item("3", PrKind::Notification),
            ],
            ..Default::default()
        };

        assert_eq!(state.todo_done_counts(), (2, 1));
        assert_eq!(state.todo_count(), 2);
        assert_eq!(state.done_count(), 1);
        assert_eq!(state.tray_title(), "PR 2");
    }

    #[test]
    fn local_done_review_requests_count_as_done() {
        let state = AppState {
            pull_requests: vec![
                PullRequestItem {
                    locally_done: true,
                    ..item("1", PrKind::ReviewRequested)
                },
                item("2", PrKind::Notification),
            ],
            ..Default::default()
        };

        assert_eq!(state.todo_done_counts(), (1, 1));
        assert_eq!(state.todo_count(), 1);
        assert_eq!(state.done_count(), 1);
    }
}
