use pullbell::model::PullRequestItem;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(super) struct NotificationTracker {
    known_actionable: HashMap<String, bool>,
    bootstrapped: bool,
}

impl NotificationTracker {
    pub(super) fn new_notifications(&mut self, items: &[PullRequestItem]) -> Vec<PullRequestItem> {
        let notifications = if self.bootstrapped {
            items
                .iter()
                .filter(|item| {
                    item.is_todo()
                        && !self
                            .known_actionable
                            .get(&item.id)
                            .copied()
                            .unwrap_or(false)
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        self.known_actionable.clear();
        self.known_actionable
            .extend(items.iter().map(|item| (item.id.clone(), item.is_todo())));
        self.bootstrapped = true;

        notifications
    }

    pub(super) fn reset(&mut self) {
        self.known_actionable.clear();
        self.bootstrapped = false;
    }

    pub(super) fn mark_non_actionable(&mut self, id: &str) {
        if self.bootstrapped {
            self.known_actionable.insert(id.to_string(), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pullbell::model::PrKind;

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
    fn reports_items_that_become_actionable() {
        let mut tracker = NotificationTracker::default();
        tracker.new_notifications(&[item("1", PrKind::Authored)]);

        let notifications = tracker.new_notifications(&[item("1", PrKind::Notification)]);

        let ids: Vec<_> = notifications.into_iter().map(|item| item.id).collect();
        assert_eq!(ids, vec!["1"]);
    }

    #[test]
    fn does_not_report_items_that_remain_actionable() {
        let mut tracker = NotificationTracker::default();
        tracker.new_notifications(&[item("1", PrKind::Notification)]);

        let notifications = tracker.new_notifications(&[item("1", PrKind::ReviewRequested)]);

        assert!(notifications.is_empty());
    }

    #[test]
    fn does_not_report_locally_done_items_as_actionable() {
        let mut tracker = NotificationTracker::default();
        tracker.new_notifications(&[item("1", PrKind::Authored)]);

        let notifications = tracker.new_notifications(&[PullRequestItem {
            locally_done: true,
            ..item("1", PrKind::ReviewRequested)
        }]);

        assert!(notifications.is_empty());
    }

    #[test]
    fn reports_item_that_updates_after_being_marked_non_actionable() {
        let mut tracker = NotificationTracker::default();
        tracker.new_notifications(&[item("1", PrKind::ReviewRequested)]);
        tracker.mark_non_actionable("1");

        let notifications = tracker.new_notifications(&[item("1", PrKind::ReviewRequested)]);

        let ids: Vec<_> = notifications.into_iter().map(|item| item.id).collect();
        assert_eq!(ids, vec!["1"]);
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
