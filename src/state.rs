use crate::model::{AvailableUpdate, PullRequestItem};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub signed_in_as: Option<String>,
    pub token_loaded: bool,
    pub is_refreshing: bool,
    pub is_checking_updates: bool,
    pub homebrew_cask_installed: bool,
    pub last_error: Option<String>,
    pub update_status: Option<String>,
    pub last_refreshed_at: Option<DateTime<Utc>>,
    pub last_update_checked_at: Option<DateTime<Utc>>,
    pub available_update: Option<AvailableUpdate>,
    pub pending_auth: Option<PendingAuth>,
    pub pull_requests: Vec<PullRequestItem>,
}

#[derive(Debug, Clone)]
pub struct PendingAuth {
    pub user_code: String,
    pub verification_uri: String,
}

impl AppState {
    pub fn tray_title(&self) -> String {
        let count = self.todo_count();
        if count == 0 {
            "PR".to_string()
        } else {
            format!("PR {count}")
        }
    }

    pub fn todo_count(&self) -> usize {
        self.pull_requests
            .iter()
            .filter(|item| item.is_todo())
            .count()
    }

    pub fn done_count(&self) -> usize {
        self.pull_requests.len().saturating_sub(self.todo_count())
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

        assert_eq!(state.todo_count(), 2);
        assert_eq!(state.done_count(), 1);
        assert_eq!(state.tray_title(), "PR 2");
    }
}
