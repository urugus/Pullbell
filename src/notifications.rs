use pullbell::model::{PrKind, PullRequestItem};
use std::collections::HashSet;

#[derive(Debug, Default)]
pub(super) struct NotificationTracker {
    known_ids: HashSet<String>,
    bootstrapped: bool,
}

impl NotificationTracker {
    pub(super) fn new_notifications(&mut self, items: &[PullRequestItem]) -> Vec<PullRequestItem> {
        let notifications = if self.bootstrapped {
            items
                .iter()
                .filter(|item| {
                    !self.known_ids.contains(&item.id)
                        && matches!(item.kind, PrKind::ReviewRequested | PrKind::Notification)
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        self.known_ids.clear();
        self.known_ids
            .extend(items.iter().map(|item| item.id.clone()));
        self.bootstrapped = true;

        notifications
    }

    pub(super) fn reset(&mut self) {
        self.known_ids.clear();
        self.bootstrapped = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    #[test]
    fn bootstraps_without_notifications() {
        let mut tracker = NotificationTracker::default();

        let notifications = tracker.new_notifications(&[item("1", PrKind::ReviewRequested)]);

        assert!(notifications.is_empty());
    }

    #[test]
    fn reports_new_actionable_items_only() {
        let mut tracker = NotificationTracker::default();
        tracker.new_notifications(&[
            item("1", PrKind::ReviewRequested),
            item("2", PrKind::Authored),
        ]);

        let notifications = tracker.new_notifications(&[
            item("1", PrKind::ReviewRequested),
            item("2", PrKind::Authored),
            item("3", PrKind::Authored),
            item("4", PrKind::Notification),
            item("5", PrKind::ReviewRequested),
        ]);

        let ids: Vec<_> = notifications.into_iter().map(|item| item.id).collect();
        assert_eq!(ids, vec!["4", "5"]);
    }

    #[test]
    fn reset_bootstraps_again() {
        let mut tracker = NotificationTracker::default();
        tracker.new_notifications(&[item("1", PrKind::ReviewRequested)]);
        tracker.reset();

        let notifications = tracker.new_notifications(&[item("2", PrKind::ReviewRequested)]);

        assert!(notifications.is_empty());
    }
}
