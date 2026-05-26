use super::AppCommand;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use pullbell::model::{PrKind, PullRequestItem};
use pullbell::state::AppState;
use pullbell::updater;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const MAX_ITEMS_PER_SECTION: usize = 12;
const MAX_MENU_LABEL_CHARS: usize = 92;

pub(super) fn rebuild(
    tray: &mut TrayIcon,
    state: &Arc<Mutex<AppState>>,
    command_map: &Arc<Mutex<HashMap<MenuId, AppCommand>>>,
) -> Result<()> {
    let snapshot = state.lock().expect("state lock").clone();
    let menu = Menu::new();
    let mut commands = HashMap::new();

    append_disabled(&menu, "Pullbell")?;
    append_disabled(&menu, &format!("Version {}", env!("CARGO_PKG_VERSION")))?;
    append_disabled(&menu, "GitHub pull requests on your menu bar")?;
    if let Some(login) = &snapshot.signed_in_as {
        append_disabled(&menu, &format!("Signed in as {login}"))?;
    } else if snapshot.token_loaded {
        append_disabled(&menu, "Signed in")?;
    } else {
        append_disabled(&menu, "Not signed in")?;
    }
    menu.append(&PredefinedMenuItem::separator())?;

    append_pinned_section(&menu, &mut commands, &snapshot)?;

    menu.append(&PredefinedMenuItem::separator())?;

    append_pr_groups(&menu, &mut commands, &snapshot.pull_requests)?;

    menu.append(&PredefinedMenuItem::separator())?;

    append_actions_section(&menu, &mut commands, &snapshot)?;

    menu.append(&PredefinedMenuItem::separator())?;

    append_command(&menu, &mut commands, "Quit Pullbell", AppCommand::Quit)?;

    *command_map.lock().expect("menu command lock") = commands;
    tray.set_title(Some(&snapshot.tray_title()));
    tray.set_tooltip(Some("Pullbell"))?;
    tray.set_menu(Some(Box::new(menu)));

    Ok(())
}

fn append_pinned_section(
    menu: &Menu,
    commands: &mut HashMap<MenuId, AppCommand>,
    snapshot: &AppState,
) -> Result<()> {
    append_disabled(menu, "Pinned")?;

    if let Some(auth) = &snapshot.pending_auth {
        append_disabled(menu, "GitHub sign-in is waiting")?;
        append_disabled(menu, "Enter this code on GitHub:")?;
        append_disabled(menu, &format!(">>> {} <<<", auth.user_code))?;
        append_command(
            menu,
            commands,
            "Copy sign-in code",
            AppCommand::CopySignInCode,
        )?;
        append_command(
            menu,
            commands,
            "Open GitHub device page",
            AppCommand::OpenUrl(auth.verification_uri.clone()),
        )?;
        return Ok(());
    }

    if let Some(update) = &snapshot.available_update {
        append_disabled(
            menu,
            &format!("Update available: v{}", update.latest_version),
        )?;
        if update.download_url.is_some() {
            append_command(menu, commands, "Install update", AppCommand::InstallUpdate)?;
        }
        append_command(
            menu,
            commands,
            "Open release page",
            AppCommand::OpenUrl(update.release_url.clone()),
        )?;
        return Ok(());
    }

    if let Some(update_status) = &snapshot.update_status {
        append_disabled(
            menu,
            &truncate_menu_label(update_status, MAX_MENU_LABEL_CHARS),
        )?;
        return Ok(());
    }

    if let Some(status) = &snapshot.last_status {
        append_disabled(menu, &truncate_menu_label(status, MAX_MENU_LABEL_CHARS))?;
        return Ok(());
    }

    if snapshot.is_checking_updates {
        append_disabled(menu, "Checking for updates...")?;
    } else if let Some(error) = &snapshot.last_error {
        append_disabled(menu, &truncate_menu_label(error, MAX_MENU_LABEL_CHARS))?;
    } else if let Some(refreshed_at) = snapshot.last_refreshed_at {
        append_disabled(
            menu,
            &format!("Last refreshed {}", refreshed_at.format("%H:%M:%S")),
        )?;
    } else if snapshot.token_loaded {
        append_disabled(menu, "Ready to watch pull requests")?;
    } else {
        append_disabled(menu, "Sign in to start watching pull requests")?;
    }

    Ok(())
}

fn append_actions_section(
    menu: &Menu,
    commands: &mut HashMap<MenuId, AppCommand>,
    snapshot: &AppState,
) -> Result<()> {
    append_disabled(menu, "Actions")?;

    if !snapshot.token_loaded && snapshot.pending_auth.is_none() {
        append_command(menu, commands, "Sign in with GitHub", AppCommand::SignIn)?;
    }

    append_command(
        menu,
        commands,
        if snapshot.is_refreshing {
            "Refreshing..."
        } else {
            "Refresh now"
        },
        AppCommand::Refresh,
    )?;
    append_command(
        menu,
        commands,
        "Open GitHub inbox",
        AppCommand::OpenUrl("https://github.com/notifications".to_string()),
    )?;

    if snapshot.available_update.is_none() {
        if snapshot.is_checking_updates {
            append_disabled(menu, "Checking for updates...")?;
        } else {
            append_command(
                menu,
                commands,
                "Check for updates",
                AppCommand::CheckForUpdates,
            )?;
        }
        append_command(
            menu,
            commands,
            "Open release page",
            AppCommand::OpenUrl(updater::RELEASES_URL.to_string()),
        )?;
    }

    if snapshot.token_loaded {
        append_command(menu, commands, "Sign out", AppCommand::SignOut)?;
    }

    Ok(())
}

fn append_command(
    menu: &Menu,
    commands: &mut HashMap<MenuId, AppCommand>,
    label: &str,
    command: AppCommand,
) -> Result<()> {
    let item = MenuItem::new(label, true, None);
    commands.insert(item.id().clone(), command);
    menu.append(&item)?;
    Ok(())
}

fn append_disabled(menu: &Menu, label: &str) -> Result<()> {
    menu.append(&MenuItem::new(label, false, None))?;
    Ok(())
}

fn append_pr_groups(
    menu: &Menu,
    commands: &mut HashMap<MenuId, AppCommand>,
    items: &[PullRequestItem],
) -> Result<()> {
    let (todo_count, done_count) = count_pr_groups(items);

    append_disabled(menu, &format!("To do ({todo_count})"))?;
    if todo_count == 0 {
        append_disabled(menu, "All caught up")?;
    } else {
        append_pr_section(menu, commands, items, PrKind::ReviewRequested)?;
        append_pr_section(menu, commands, items, PrKind::Notification)?;
    }

    menu.append(&PredefinedMenuItem::separator())?;
    append_disabled(menu, &format!("Done ({done_count})"))?;
    if done_count == 0 {
        append_disabled(menu, "No open PRs being tracked")?;
    } else {
        append_pr_section(menu, commands, items, PrKind::Authored)?;
    }

    Ok(())
}

fn append_pr_section(
    menu: &Menu,
    commands: &mut HashMap<MenuId, AppCommand>,
    items: &[PullRequestItem],
    kind: PrKind,
) -> Result<()> {
    let section_count = items.iter().filter(|item| item.kind == kind).count();
    if section_count == 0 {
        return Ok(());
    }

    let now = Utc::now();
    for item in items
        .iter()
        .filter(|item| item.kind == kind)
        .take(MAX_ITEMS_PER_SECTION)
    {
        append_command(
            menu,
            commands,
            &truncate_menu_label(&neat_item_label(item, now), MAX_MENU_LABEL_CHARS),
            AppCommand::OpenUrl(item.url.clone()),
        )?;
    }

    let hidden_count = section_count.saturating_sub(MAX_ITEMS_PER_SECTION);
    if hidden_count > 0 {
        append_disabled(menu, &format!("...and {hidden_count} more"))?;
    }

    Ok(())
}

fn count_pr_groups(items: &[PullRequestItem]) -> (usize, usize) {
    items.iter().fold((0, 0), |(todo, done), item| {
        if item.is_todo() {
            (todo + 1, done)
        } else {
            (todo, done + 1)
        }
    })
}

fn neat_item_label(item: &PullRequestItem, now: DateTime<Utc>) -> String {
    let repo = short_repo_name(&item.repo);
    let age = item
        .updated_at
        .map(|updated_at| relative_age(updated_at, now))
        .unwrap_or_else(|| "unknown".to_string());

    format!(
        "{} in {} #{} - {} - {}",
        activity_label(&item.kind),
        repo,
        item.number,
        age,
        item.title
    )
}

fn activity_label(kind: &PrKind) -> &'static str {
    match kind {
        PrKind::ReviewRequested => "Review requested",
        PrKind::Notification => "Unread notification",
        PrKind::Authored => "Authored",
    }
}

fn short_repo_name(repo: &str) -> &str {
    repo.rsplit('/').next().unwrap_or(repo)
}

fn relative_age(updated_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let age = now.signed_duration_since(updated_at);
    let age = if age < Duration::zero() {
        Duration::zero()
    } else {
        age
    };

    if age.num_days() >= 1 {
        format!("{}d", age.num_days())
    } else if age.num_hours() >= 1 {
        format!("{}h", age.num_hours())
    } else if age.num_minutes() >= 1 {
        format!("{}m", age.num_minutes())
    } else {
        "now".to_string()
    }
}

pub(super) fn build_tray() -> Result<TrayIcon> {
    TrayIconBuilder::new()
        .with_icon(build_icon()?)
        .with_title("PR")
        .with_tooltip("Pullbell")
        .with_menu_on_left_click(false)
        .build()
        .context("building macOS menu bar icon")
}

fn build_icon() -> Result<Icon> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let in_dot = ((3..=5).contains(&x) && (3..=5).contains(&y))
                || ((10..=12).contains(&x) && (10..=12).contains(&y));
            let in_line = (x == 4 && y > 5 && y < 12)
                || (y == 11 && x > 4 && x < 10)
                || (x == 11 && y > 5 && y < 10);
            let alpha = if in_dot || in_line { 255 } else { 0 };
            rgba.extend_from_slice(&[0, 0, 0, alpha]);
        }
    }
    Icon::from_rgba(rgba, 16, 16).context("building tray icon")
}

fn truncate_menu_label(label: &str, max_chars: usize) -> String {
    let count = label.chars().count();
    if count <= max_chars {
        label.to_string()
    } else if max_chars <= 3 {
        "...".chars().take(max_chars).collect()
    } else {
        let mut value: String = label.chars().take(max_chars.saturating_sub(3)).collect();
        value.push_str("...");
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn item(kind: PrKind, updated_at: i64) -> PullRequestItem {
        PullRequestItem {
            id: "owner/repo#42".to_string(),
            repo: "owner/repo".to_string(),
            title: "Tighten notification layout".to_string(),
            url: "https://github.com/owner/repo/pull/42".to_string(),
            number: 42,
            updated_at: Some(Utc.timestamp_opt(updated_at, 0).unwrap()),
            kind,
            notification_thread_id: None,
            author: None,
            reason: None,
            preview: None,
        }
    }

    #[test]
    fn formats_menu_items_like_compact_notification_rows() {
        let now = Utc.timestamp_opt(7_200, 0).unwrap();

        assert_eq!(
            neat_item_label(&item(PrKind::ReviewRequested, 0), now),
            "Review requested in repo #42 - 2h - Tighten notification layout"
        );
    }

    #[test]
    fn relative_age_uses_short_units() {
        let now = Utc.timestamp_opt(172_800, 0).unwrap();

        assert_eq!(
            relative_age(Utc.timestamp_opt(172_740, 0).unwrap(), now),
            "1m"
        );
        assert_eq!(
            relative_age(Utc.timestamp_opt(165_600, 0).unwrap(), now),
            "2h"
        );
        assert_eq!(relative_age(Utc.timestamp_opt(0, 0).unwrap(), now), "2d");
        assert_eq!(
            relative_age(Utc.timestamp_opt(172_900, 0).unwrap(), now),
            "now"
        );
    }

    #[test]
    fn truncates_menu_labels_within_the_requested_width() {
        assert_eq!(truncate_menu_label("abcdef", 6), "abcdef");
        assert_eq!(truncate_menu_label("abcdef", 5), "ab...");
        assert_eq!(truncate_menu_label("abcdef", 3), "...");
        assert_eq!(truncate_menu_label("abcdef", 2), "..");
        assert_eq!(truncate_menu_label("abcdef", 1), ".");
        assert_eq!(truncate_menu_label("abcdef", 0), "");
    }
}
