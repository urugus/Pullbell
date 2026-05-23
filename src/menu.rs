use super::AppCommand;
use anyhow::{Context, Result};
use pullbell::model::{PrKind, PullRequestItem};
use pullbell::state::AppState;
use pullbell::updater;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const MAX_ITEMS_PER_SECTION: usize = 12;

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
    if let Some(login) = &snapshot.signed_in_as {
        append_disabled(&menu, &format!("Signed in as {login}"))?;
    } else if snapshot.token_loaded {
        append_disabled(&menu, "Signed in")?;
    } else {
        append_disabled(&menu, "Not signed in")?;
    }
    menu.append(&PredefinedMenuItem::separator())?;

    if let Some(auth) = &snapshot.pending_auth {
        append_disabled(&menu, "GitHub sign-in is waiting")?;
        append_disabled(&menu, "Enter this code on GitHub:")?;
        append_disabled(&menu, &format!(">>> {} <<<", auth.user_code))?;
        append_command(
            &menu,
            &mut commands,
            "Copy sign-in code",
            AppCommand::CopySignInCode,
        )?;
        append_command(
            &menu,
            &mut commands,
            "Open GitHub device page",
            AppCommand::OpenUrl(auth.verification_uri.clone()),
        )?;
        menu.append(&PredefinedMenuItem::separator())?;
    }

    append_disabled(
        &menu,
        &format!(
            "ToDo {} / Done {}",
            snapshot.todo_count(),
            snapshot.done_count()
        ),
    )?;

    if !snapshot.token_loaded && snapshot.pending_auth.is_none() {
        append_command(
            &menu,
            &mut commands,
            "Sign in with GitHub",
            AppCommand::SignIn,
        )?;
    }

    append_command(
        &menu,
        &mut commands,
        if snapshot.is_refreshing {
            "Refreshing..."
        } else {
            "Refresh now"
        },
        AppCommand::Refresh,
    )?;
    append_command(
        &menu,
        &mut commands,
        "Open GitHub inbox",
        AppCommand::OpenUrl("https://github.com/notifications".to_string()),
    )?;

    menu.append(&PredefinedMenuItem::separator())?;

    if let Some(update) = &snapshot.available_update {
        append_disabled(
            &menu,
            &format!("Update available: v{}", update.latest_version),
        )?;
        if snapshot.homebrew_cask_installed {
            append_command(
                &menu,
                &mut commands,
                "Update with Homebrew",
                AppCommand::UpdateWithHomebrew,
            )?;
        }
        append_command(
            &menu,
            &mut commands,
            "Open release page",
            AppCommand::OpenUrl(update.release_url.clone()),
        )?;
    } else if snapshot.is_checking_updates {
        append_disabled(&menu, "Checking for updates...")?;
    } else {
        append_command(
            &menu,
            &mut commands,
            "Check for updates",
            AppCommand::CheckForUpdates,
        )?;
        append_command(
            &menu,
            &mut commands,
            "Open release page",
            AppCommand::OpenUrl(updater::RELEASES_URL.to_string()),
        )?;
    }

    if let Some(update_status) = &snapshot.update_status {
        append_disabled(&menu, &truncate_menu_label(update_status, 90))?;
    }

    menu.append(&PredefinedMenuItem::separator())?;

    append_pr_groups(&menu, &mut commands, &snapshot.pull_requests)?;

    menu.append(&PredefinedMenuItem::separator())?;

    if let Some(error) = &snapshot.last_error {
        append_disabled(&menu, &truncate_menu_label(error, 90))?;
    } else if let Some(refreshed_at) = snapshot.last_refreshed_at {
        append_disabled(
            &menu,
            &format!("Last refreshed {}", refreshed_at.format("%H:%M:%S")),
        )?;
    }

    if snapshot.token_loaded {
        append_command(&menu, &mut commands, "Sign out", AppCommand::SignOut)?;
    }
    append_command(&menu, &mut commands, "Quit", AppCommand::Quit)?;

    *command_map.lock().expect("menu command lock") = commands;
    tray.set_title(Some(&snapshot.tray_title()));
    tray.set_tooltip(Some("Pullbell"))?;
    tray.set_menu(Some(Box::new(menu)));

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
    let todo_count = items.iter().filter(|item| item.is_todo()).count();
    let done_count = items.len().saturating_sub(todo_count);

    append_disabled(menu, &format!("ToDo ({todo_count})"))?;
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
    let section_items: Vec<_> = items.iter().filter(|item| item.kind == kind).collect();
    if section_items.is_empty() {
        return Ok(());
    }

    append_disabled(menu, kind.label())?;
    for item in section_items.iter().take(MAX_ITEMS_PER_SECTION) {
        append_command(
            menu,
            commands,
            &truncate_menu_label(&item.display_title(), 80),
            AppCommand::OpenUrl(item.url.clone()),
        )?;
    }

    let hidden_count = section_items.len().saturating_sub(MAX_ITEMS_PER_SECTION);
    if hidden_count > 0 {
        append_disabled(menu, &format!("...and {hidden_count} more"))?;
    }

    Ok(())
}

pub(super) fn build_tray() -> Result<TrayIcon> {
    TrayIconBuilder::new()
        .with_icon(build_icon()?)
        .with_title("PR")
        .with_tooltip("Pullbell")
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
    } else {
        let mut value: String = label.chars().take(max_chars.saturating_sub(1)).collect();
        value.push_str("...");
        value
    }
}
