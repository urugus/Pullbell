use super::AppCommand;
use anyhow::{Context, Result};
use pullbell::state::AppState;
use pullbell::updater;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

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

    if !snapshot.token_loaded {
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
        "Open GitHub notifications",
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

    if snapshot.pull_requests.is_empty() {
        append_disabled(&menu, "No pull requests need attention")?;
    } else {
        let mut current_label = "";
        for item in snapshot.pull_requests.iter().take(20) {
            if item.kind.label() != current_label {
                current_label = item.kind.label();
                append_disabled(&menu, current_label)?;
            }
            append_command(
                &menu,
                &mut commands,
                &truncate_menu_label(&item.display_title(), 80),
                AppCommand::OpenUrl(item.url.clone()),
            )?;
        }
    }

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
