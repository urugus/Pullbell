use crate::model::PullRequestItem;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub signed_in_as: Option<String>,
    pub token_loaded: bool,
    pub is_refreshing: bool,
    pub last_error: Option<String>,
    pub last_refreshed_at: Option<DateTime<Utc>>,
    pub pull_requests: Vec<PullRequestItem>,
}

impl AppState {
    pub fn tray_title(&self) -> String {
        let count = self.pull_requests.len();
        if count == 0 {
            "PR".to_string()
        } else {
            format!("PR {count}")
        }
    }
}
