use anyhow::{Context, Result};
use chrono::Utc;
use mac_notification_sys::send_notification;
use pullbell::auth::OAuthDeviceClient;
use pullbell::github::GitHubClient;
use pullbell::model::{PrKind, PullRequestItem};
use pullbell::state::AppState;
use pullbell::storage;
use pullbell::updater;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const POLL_INTERVAL: Duration = Duration::from_secs(300);
const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60 * 12);
const DEFAULT_CLIENT_ID: Option<&str> = option_env!("PULLBELL_DEFAULT_CLIENT_ID");

#[derive(Debug, Clone)]
enum AppEvent {
    StateChanged,
    Notify(PullRequestItem),
}

#[derive(Debug, Clone)]
enum AppCommand {
    SignIn,
    Refresh,
    CheckForUpdates,
    UpdateWithHomebrew,
    SignOut,
    OpenUrl(String),
    Quit,
}

fn main() -> Result<()> {
    let runtime = Runtime::new().context("starting async runtime")?;
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let state = Arc::new(Mutex::new(AppState::default()));
    let (command_tx, command_rx) = mpsc::unbounded_channel();

    let client_id = load_client_id();
    let menu_commands = Arc::new(Mutex::new(HashMap::<MenuId, AppCommand>::new()));
    let mut tray = build_tray()?;

    if storage::load_token()?.is_some() {
        state.lock().expect("state lock").token_loaded = true;
        command_tx.send(AppCommand::Refresh).ok();
    }

    runtime.spawn(run_worker(
        command_rx,
        Arc::clone(&state),
        proxy.clone(),
        client_id,
    ));

    rebuild_menu(&mut tray, &state, &menu_commands, &command_tx)?;

    let menu_events = MenuEvent::receiver();
    event_loop.run(move |event, _, control_flow| {
        *control_flow =
            ControlFlow::WaitUntil(std::time::Instant::now() + Duration::from_millis(250));

        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                while let Ok(menu_event) = menu_events.try_recv() {
                    let command = menu_commands
                        .lock()
                        .expect("menu command lock")
                        .get(&menu_event.id)
                        .cloned();

                    if let Some(command) = command {
                        match command {
                            AppCommand::OpenUrl(url) => {
                                let _ = open::that(url);
                            }
                            AppCommand::Quit => {
                                *control_flow = ControlFlow::Exit;
                            }
                            other => {
                                let _ = command_tx.send(other);
                            }
                        }
                    }
                }
            }
            Event::UserEvent(AppEvent::StateChanged) => {
                if let Err(error) = rebuild_menu(&mut tray, &state, &menu_commands, &command_tx) {
                    eprintln!("failed to rebuild menu: {error:#}");
                }
            }
            Event::UserEvent(AppEvent::Notify(item)) => {
                let _ = send_notification(
                    "Pullbell",
                    Some(item.kind.label()),
                    &item.display_title(),
                    None,
                );
            }
            _ => {}
        }
    });
}

async fn run_worker(
    mut command_rx: mpsc::UnboundedReceiver<AppCommand>,
    state: Arc<Mutex<AppState>>,
    proxy: EventLoopProxy<AppEvent>,
    client_id: Option<String>,
) {
    let mut known_ids = HashSet::new();
    let mut bootstrapped = false;
    let mut poll = tokio::time::interval(POLL_INTERVAL);
    let mut update_poll = tokio::time::interval(UPDATE_CHECK_INTERVAL);

    loop {
        tokio::select! {
            _ = poll.tick() => {
                refresh(&state, &proxy, &mut known_ids, &mut bootstrapped).await;
            }
            _ = update_poll.tick() => {
                check_for_updates(&state, &proxy, false).await;
            }
            Some(command) = command_rx.recv() => {
                match command {
                    AppCommand::SignIn => {
                        sign_in(&state, &proxy, client_id.clone()).await;
                        refresh(&state, &proxy, &mut known_ids, &mut bootstrapped).await;
                    }
                    AppCommand::Refresh => {
                        refresh(&state, &proxy, &mut known_ids, &mut bootstrapped).await;
                    }
                    AppCommand::CheckForUpdates => {
                        check_for_updates(&state, &proxy, true).await;
                    }
                    AppCommand::UpdateWithHomebrew => {
                        start_homebrew_update(&state, &proxy);
                    }
                    AppCommand::SignOut => {
                        if let Err(error) = storage::delete_token() {
                            set_error(&state, format!("{error:#}"));
                        }
                        let homebrew_cask_installed = {
                            state
                                .lock()
                                .expect("state lock")
                                .homebrew_cask_installed
                        };
                        let mut guard = state.lock().expect("state lock");
                        *guard = AppState::default();
                        guard.homebrew_cask_installed = homebrew_cask_installed;
                        known_ids.clear();
                        bootstrapped = false;
                        let _ = proxy.send_event(AppEvent::StateChanged);
                    }
                    AppCommand::OpenUrl(_) => {}
                    AppCommand::Quit => {}
                }
            }
        }
    }
}

async fn sign_in(
    state: &Arc<Mutex<AppState>>,
    proxy: &EventLoopProxy<AppEvent>,
    client_id: Option<String>,
) {
    let Some(client_id) = client_id else {
        set_error(
            state,
            "OAuth client ID is not configured. Set PULLBELL_CLIENT_ID or ~/.config/pullbell/client_id.".to_string(),
        );
        let _ = proxy.send_event(AppEvent::StateChanged);
        return;
    };

    clear_error(state);
    let client = OAuthDeviceClient::new(client_id);
    let code = match client.request_device_code().await {
        Ok(code) => code,
        Err(error) => {
            set_error(state, format!("{error:#}"));
            let _ = proxy.send_event(AppEvent::StateChanged);
            return;
        }
    };

    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(code.user_code.clone());
    }

    {
        let mut guard = state.lock().expect("state lock");
        guard.last_error = Some(format!(
            "GitHub sign-in opened. Enter code {}. The code was copied to the clipboard.",
            code.user_code
        ));
    }
    let _ = proxy.send_event(AppEvent::StateChanged);
    let _ = open::that(&code.verification_uri);

    match client.wait_for_token(&code).await {
        Ok(token) => {
            if let Err(error) = storage::save_token(&token.token) {
                set_error(state, format!("{error:#}"));
            } else {
                let mut guard = state.lock().expect("state lock");
                guard.token_loaded = true;
                guard.last_error = None;
            }
        }
        Err(error) => set_error(state, format!("{error:#}")),
    }
    let _ = proxy.send_event(AppEvent::StateChanged);
}

async fn refresh(
    state: &Arc<Mutex<AppState>>,
    proxy: &EventLoopProxy<AppEvent>,
    known_ids: &mut HashSet<String>,
    bootstrapped: &mut bool,
) {
    {
        let mut guard = state.lock().expect("state lock");
        guard.is_refreshing = true;
        guard.last_error = None;
    }
    let _ = proxy.send_event(AppEvent::StateChanged);

    let token = match storage::load_token() {
        Ok(Some(token)) => token,
        Ok(None) => {
            let mut guard = state.lock().expect("state lock");
            guard.is_refreshing = false;
            guard.token_loaded = false;
            guard.pull_requests.clear();
            let _ = proxy.send_event(AppEvent::StateChanged);
            return;
        }
        Err(error) => {
            set_error(state, format!("{error:#}"));
            let _ = proxy.send_event(AppEvent::StateChanged);
            return;
        }
    };

    let client = GitHubClient::new(token);
    let viewer = client.viewer().await;
    let pull_requests = client.pull_requests().await;

    match (viewer, pull_requests) {
        (Ok(viewer), Ok(items)) => {
            for item in &items {
                if *bootstrapped
                    && !known_ids.contains(&item.id)
                    && matches!(item.kind, PrKind::ReviewRequested | PrKind::Notification)
                {
                    let _ = proxy.send_event(AppEvent::Notify(item.clone()));
                }
            }

            known_ids.clear();
            known_ids.extend(items.iter().map(|item| item.id.clone()));
            *bootstrapped = true;

            let mut guard = state.lock().expect("state lock");
            guard.signed_in_as = Some(viewer.login);
            guard.token_loaded = true;
            guard.is_refreshing = false;
            guard.last_refreshed_at = Some(Utc::now());
            guard.last_error = None;
            guard.pull_requests = items;
        }
        (Err(error), _) | (_, Err(error)) => {
            let mut guard = state.lock().expect("state lock");
            guard.is_refreshing = false;
            guard.last_error = Some(format!("{error:#}"));
        }
    }

    let _ = proxy.send_event(AppEvent::StateChanged);
}

async fn check_for_updates(
    state: &Arc<Mutex<AppState>>,
    proxy: &EventLoopProxy<AppEvent>,
    show_status: bool,
) {
    {
        let mut guard = state.lock().expect("state lock");
        guard.is_checking_updates = true;
        if show_status {
            guard.update_status = None;
        }
    }
    let _ = proxy.send_event(AppEvent::StateChanged);

    let homebrew_cask_installed = updater::is_homebrew_cask_installed();
    let result = updater::check_latest_release(env!("CARGO_PKG_VERSION")).await;
    let mut guard = state.lock().expect("state lock");
    guard.is_checking_updates = false;
    guard.homebrew_cask_installed = homebrew_cask_installed;
    guard.last_update_checked_at = Some(Utc::now());

    match result {
        Ok(update) => {
            guard.available_update = update;
            if guard.available_update.is_some() {
                guard.update_status = None;
            } else if show_status {
                guard.update_status = Some("Pullbell is up to date".to_string());
            }
        }
        Err(error) => {
            if show_status {
                guard.update_status = Some(format!("Update check failed: {error:#}"));
            }
        }
    }

    let _ = proxy.send_event(AppEvent::StateChanged);
}

fn start_homebrew_update(state: &Arc<Mutex<AppState>>, proxy: &EventLoopProxy<AppEvent>) {
    match updater::start_homebrew_update() {
        Ok(()) => {
            state.lock().expect("state lock").update_status =
                Some("Homebrew update started in Terminal".to_string());
        }
        Err(error) => {
            state.lock().expect("state lock").update_status =
                Some(format!("Homebrew update could not start: {error:#}"));
        }
    }

    let _ = proxy.send_event(AppEvent::StateChanged);
}

fn rebuild_menu(
    tray: &mut TrayIcon,
    state: &Arc<Mutex<AppState>>,
    command_map: &Arc<Mutex<HashMap<MenuId, AppCommand>>>,
    command_tx: &mpsc::UnboundedSender<AppCommand>,
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

    let _ = command_tx;

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

fn build_tray() -> Result<TrayIcon> {
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

fn clear_error(state: &Arc<Mutex<AppState>>) {
    state.lock().expect("state lock").last_error = None;
}

fn set_error(state: &Arc<Mutex<AppState>>, error: String) {
    let mut guard = state.lock().expect("state lock");
    guard.is_refreshing = false;
    guard.last_error = Some(error);
}

fn load_client_id() -> Option<String> {
    std::env::var("PULLBELL_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            let path = dirs::config_dir()?.join("pullbell/client_id");
            fs::read_to_string(path)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            DEFAULT_CLIENT_ID
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}
