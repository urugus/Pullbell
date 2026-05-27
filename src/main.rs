use anyhow::{Context, Result};
use chrono::Utc;
use mac_notification_sys::send_notification;
use pullbell::auth::OAuthDeviceClient;
use pullbell::github::GitHubClient;
use pullbell::model::PullRequestItem;
use pullbell::state::{AppState, PendingAuth};
use pullbell::storage;
use pullbell::updater;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tray_icon::menu::{MenuEvent, MenuId};
use tray_icon::{MouseButton, MouseButtonState, Rect, TrayIconEvent};

mod menu;
mod notifications;
mod panel;

use notifications::NotificationTracker;

const POLL_INTERVAL: Duration = Duration::from_secs(300);
const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60 * 12);
const DEFAULT_CLIENT_ID: &str = "Ov23liYs8QgtSc19mkZs";

#[derive(Debug, Clone)]
enum AppEvent {
    StateChanged,
    Notify(PullRequestItem),
    PanelCommand(String),
}

#[derive(Debug, Clone)]
enum AppCommand {
    SignIn,
    CopySignInCode,
    Refresh,
    CheckForUpdates,
    InstallUpdate,
    SignInFinished {
        attempt_id: u64,
        token: Option<String>,
    },
    SignOut,
    MarkNotificationDone(String),
    MuteNotification(String),
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
    let mut tray = menu::build_tray()?;
    let initial_snapshot = state.lock().expect("state lock").clone();
    let mut panel = panel::Panel::new(&event_loop, proxy.clone(), &initial_snapshot)?;
    let mut last_tray_rect = None::<Rect>;

    let initial_token = storage::load_token()?;
    if initial_token.is_some() {
        state.lock().expect("state lock").token_loaded = true;
        command_tx.send(AppCommand::Refresh).ok();
    }

    runtime.spawn(run_worker(
        command_rx,
        command_tx.clone(),
        Arc::clone(&state),
        proxy.clone(),
        client_id,
        initial_token,
    ));

    menu::rebuild(&mut tray, &state, &menu_commands)?;

    let menu_events = MenuEvent::receiver();
    let tray_events = TrayIconEvent::receiver();
    event_loop.run(move |event, _, control_flow| {
        *control_flow =
            ControlFlow::WaitUntil(std::time::Instant::now() + Duration::from_millis(250));

        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                while let Ok(tray_event) = tray_events.try_recv() {
                    if let TrayIconEvent::Click {
                        rect,
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = tray_event
                    {
                        last_tray_rect = Some(rect);
                        panel.toggle_near(rect);
                    }
                }

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
                            AppCommand::CopySignInCode => {
                                let code = state
                                    .lock()
                                    .expect("state lock")
                                    .pending_auth
                                    .as_ref()
                                    .map(|auth| auth.user_code.clone());

                                if let (Some(code), Ok(mut clipboard)) =
                                    (code, arboard::Clipboard::new())
                                {
                                    let _ = clipboard.set_text(code);
                                }
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
                if let Err(error) = menu::rebuild(&mut tray, &state, &menu_commands) {
                    eprintln!("failed to rebuild menu: {error:#}");
                }
                let snapshot = state.lock().expect("state lock").clone();
                if let Err(error) = panel.update(&snapshot) {
                    eprintln!("failed to update panel: {error:#}");
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
            Event::Opened { urls } if urls.iter().any(is_pullbell_show_url) => {
                panel.show_near_or_default(last_tray_rect);
            }
            Event::UserEvent(AppEvent::PanelCommand(command)) => {
                if command == "hide" {
                    panel.hide();
                    return;
                }

                if let Some(action) = command.strip_prefix("missing-thread:") {
                    set_error(
                        &state,
                        format!(
                            "{} is available for GitHub notification threads only.",
                            notification_action_label(action)
                        ),
                    );
                    let _ = proxy.send_event(AppEvent::StateChanged);
                    return;
                }

                if let Some(command) = panel_command(&state, command) {
                    match command {
                        AppCommand::OpenUrl(url) => {
                            let _ = open::that(url);
                        }
                        AppCommand::CopySignInCode => {
                            let code = state
                                .lock()
                                .expect("state lock")
                                .pending_auth
                                .as_ref()
                                .map(|auth| auth.user_code.clone());

                            if let (Some(code), Ok(mut clipboard)) =
                                (code, arboard::Clipboard::new())
                            {
                                let _ = clipboard.set_text(code);
                            }
                        }
                        AppCommand::SignOut => {
                            panel.hide();
                            let _ = command_tx.send(AppCommand::SignOut);
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
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Focused(false),
                ..
            } if window_id == panel.window_id() => {
                panel.hide();
            }
            _ => {}
        }
    });
}

fn is_pullbell_show_url(url: &url::Url) -> bool {
    url.scheme() == "pullbell"
        && url.host_str() == Some("show")
        && matches!(url.path(), "" | "/")
        && url.query().is_none()
        && url.fragment().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_show_deeplink_only() {
        assert!(is_pullbell_show_url(
            &url::Url::parse("pullbell://show").unwrap()
        ));
        assert!(!is_pullbell_show_url(
            &url::Url::parse("pullbell://toggle").unwrap()
        ));
        assert!(!is_pullbell_show_url(
            &url::Url::parse("https://github.com/notifications").unwrap()
        ));
        assert!(!is_pullbell_show_url(
            &url::Url::parse("pullbell://show?mode=toggle").unwrap()
        ));
    }
}

fn panel_command(state: &Arc<Mutex<AppState>>, command: String) -> Option<AppCommand> {
    match command.as_str() {
        "signin" => Some(AppCommand::SignIn),
        "copy-signin-code" => Some(AppCommand::CopySignInCode),
        "refresh" => Some(AppCommand::Refresh),
        "check-updates" => Some(AppCommand::CheckForUpdates),
        "install-update" => Some(AppCommand::InstallUpdate),
        "signout" => Some(AppCommand::SignOut),
        "inbox" => Some(AppCommand::OpenUrl(
            "https://github.com/notifications".to_string(),
        )),
        "quit" => Some(AppCommand::Quit),
        _ => {
            if let Some(thread_id) = command.strip_prefix("done:").filter(|id| is_thread_id(id)) {
                return Some(AppCommand::MarkNotificationDone(thread_id.to_string()));
            }

            if let Some(thread_id) = command.strip_prefix("mute:").filter(|id| is_thread_id(id)) {
                return Some(AppCommand::MuteNotification(thread_id.to_string()));
            }

            command
                .strip_prefix("open:")
                .filter(|url| {
                    url.starts_with("https://github.com/")
                        || state
                            .lock()
                            .expect("state lock")
                            .pending_auth
                            .as_ref()
                            .is_some_and(|auth| auth.verification_uri == *url)
                        || pullbell::updater::RELEASES_URL == *url
                })
                .map(|url| AppCommand::OpenUrl(url.to_string()))
        }
    }
}

fn is_thread_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn notification_action_label(action: &str) -> &'static str {
    match action {
        "done" => "Done",
        "mute" => "Mute",
        _ => "This action",
    }
}

async fn run_worker(
    mut command_rx: mpsc::UnboundedReceiver<AppCommand>,
    command_tx: mpsc::UnboundedSender<AppCommand>,
    state: Arc<Mutex<AppState>>,
    proxy: EventLoopProxy<AppEvent>,
    client_id: Option<String>,
    mut token_cache: Option<String>,
) {
    let mut notification_tracker = NotificationTracker::default();
    let mut next_sign_in_attempt_id = 1;
    let mut active_sign_in_attempt = None::<u64>;
    let mut poll =
        tokio::time::interval_at(tokio::time::Instant::now() + POLL_INTERVAL, POLL_INTERVAL);
    let mut update_poll = tokio::time::interval(UPDATE_CHECK_INTERVAL);

    loop {
        tokio::select! {
            _ = poll.tick() => {
                refresh(&state, &proxy, &mut notification_tracker, token_cache.as_deref()).await;
            }
            _ = update_poll.tick() => {
                spawn_update_check(&state, &proxy, false);
            }
            Some(command) = command_rx.recv() => {
                match command {
                    AppCommand::SignIn => {
                        if active_sign_in_attempt.is_some() {
                            let mut guard = state.lock().expect("state lock");
                            guard.last_status = Some("GitHub sign-in is already in progress.".to_string());
                            let _ = proxy.send_event(AppEvent::StateChanged);
                            continue;
                        }

                        let attempt_id = next_sign_in_attempt_id;
                        next_sign_in_attempt_id += 1;
                        active_sign_in_attempt = Some(attempt_id);
                        let state = Arc::clone(&state);
                        let proxy = proxy.clone();
                        let client_id = client_id.clone();
                        let command_tx = command_tx.clone();
                        tokio::spawn(async move {
                            let token = sign_in(&state, &proxy, client_id).await;
                            let _ = command_tx.send(AppCommand::SignInFinished { attempt_id, token });
                        });
                    }
                    AppCommand::SignInFinished { attempt_id, token } => {
                        if active_sign_in_attempt != Some(attempt_id) {
                            continue;
                        }
                        active_sign_in_attempt = None;

                        if let Some(token) = token {
                            token_cache = Some(token);
                            refresh(&state, &proxy, &mut notification_tracker, token_cache.as_deref()).await;
                        }
                    }
                    AppCommand::Refresh => {
                        refresh(&state, &proxy, &mut notification_tracker, token_cache.as_deref()).await;
                    }
                    AppCommand::CheckForUpdates => {
                        spawn_update_check(&state, &proxy, true);
                    }
                    AppCommand::InstallUpdate => {
                        start_app_update(&state, &proxy);
                    }
                    AppCommand::SignOut => {
                        active_sign_in_attempt = None;
                        if let Err(error) = storage::delete_token() {
                            set_error(&state, format!("{error:#}"));
                        }
                        let mut guard = state.lock().expect("state lock");
                        *guard = AppState::default();
                        token_cache = None;
                        notification_tracker.reset();
                        let _ = proxy.send_event(AppEvent::StateChanged);
                    }
                    AppCommand::MarkNotificationDone(thread_id) => {
                        act_on_notification_thread(
                            &state,
                            &proxy,
                            &mut notification_tracker,
                            token_cache.as_deref(),
                            &thread_id,
                            NotificationThreadAction::Done,
                        )
                        .await;
                    }
                    AppCommand::MuteNotification(thread_id) => {
                        act_on_notification_thread(
                            &state,
                            &proxy,
                            &mut notification_tracker,
                            token_cache.as_deref(),
                            &thread_id,
                            NotificationThreadAction::Mute,
                        )
                        .await;
                    }
                    AppCommand::OpenUrl(_) => {}
                    AppCommand::CopySignInCode => {}
                    AppCommand::Quit => {}
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum NotificationThreadAction {
    Done,
    Mute,
}

impl NotificationThreadAction {
    fn label(self) -> &'static str {
        match self {
            Self::Done => "Done",
            Self::Mute => "Mute",
        }
    }
}

fn spawn_update_check(
    state: &Arc<Mutex<AppState>>,
    proxy: &EventLoopProxy<AppEvent>,
    show_status: bool,
) {
    {
        let mut guard = state.lock().expect("state lock");
        if guard.is_checking_updates {
            if show_status {
                guard.update_status = Some("Update check already in progress".to_string());
            }
            let _ = proxy.send_event(AppEvent::StateChanged);
            return;
        }

        guard.is_checking_updates = true;
        if show_status {
            guard.update_status = None;
        }
    }
    let _ = proxy.send_event(AppEvent::StateChanged);

    let state = Arc::clone(state);
    let proxy = proxy.clone();
    tokio::spawn(async move {
        check_for_updates(&state, &proxy, show_status).await;
    });
}

async fn sign_in(
    state: &Arc<Mutex<AppState>>,
    proxy: &EventLoopProxy<AppEvent>,
    client_id: Option<String>,
) -> Option<String> {
    let Some(client_id) = client_id else {
        set_error(
            state,
            "OAuth client ID is not configured. Set PULLBELL_CLIENT_ID or ~/.config/pullbell/client_id.".to_string(),
        );
        let _ = proxy.send_event(AppEvent::StateChanged);
        return None;
    };

    clear_error(state);
    let client = OAuthDeviceClient::new(client_id);
    let code = match client.request_device_code().await {
        Ok(code) => code,
        Err(error) => {
            set_error(state, format!("{error:#}"));
            let _ = proxy.send_event(AppEvent::StateChanged);
            return None;
        }
    };

    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(code.user_code.clone());
    }

    {
        let mut guard = state.lock().expect("state lock");
        guard.pending_auth = Some(PendingAuth {
            user_code: code.user_code.clone(),
            verification_uri: code.verification_uri.clone(),
        });
        guard.last_error = Some(format!(
            "Enter GitHub code {}. The code was copied to the clipboard.",
            code.user_code
        ));
    }
    let _ = proxy.send_event(AppEvent::StateChanged);
    let _ = send_notification(
        "Pullbell GitHub Sign-in",
        Some(&format!("Code {}", code.user_code)),
        "Enter this code on the GitHub device authorization page. It was copied to the clipboard.",
        None,
    );
    let _ = open::that(&code.verification_uri);

    match client.wait_for_token(&code).await {
        Ok(token) => {
            if let Err(error) = storage::save_token(&token.token) {
                set_error(state, format!("{error:#}"));
                let _ = proxy.send_event(AppEvent::StateChanged);
                None
            } else {
                let mut guard = state.lock().expect("state lock");
                guard.token_loaded = true;
                guard.last_error = None;
                guard.pending_auth = None;
                let _ = proxy.send_event(AppEvent::StateChanged);
                Some(token.token)
            }
        }
        Err(error) => {
            set_error(state, format!("{error:#}"));
            state.lock().expect("state lock").pending_auth = None;
            let _ = proxy.send_event(AppEvent::StateChanged);
            None
        }
    }
}

async fn refresh(
    state: &Arc<Mutex<AppState>>,
    proxy: &EventLoopProxy<AppEvent>,
    notification_tracker: &mut NotificationTracker,
    token: Option<&str>,
) {
    {
        let mut guard = state.lock().expect("state lock");
        guard.is_refreshing = true;
        guard.last_error = None;
        guard.last_status = None;
    }
    let _ = proxy.send_event(AppEvent::StateChanged);

    let Some(token) = token else {
        let mut guard = state.lock().expect("state lock");
        guard.is_refreshing = false;
        guard.token_loaded = false;
        guard.pull_requests.clear();
        let _ = proxy.send_event(AppEvent::StateChanged);
        return;
    };

    let client = GitHubClient::new(token);
    match client.viewer().await {
        Ok(viewer) => match client.pull_requests_for(&viewer.login).await {
            Ok(items) => {
                for item in notification_tracker.new_notifications(&items) {
                    let _ = proxy.send_event(AppEvent::Notify(item));
                }

                let mut guard = state.lock().expect("state lock");
                guard.signed_in_as = Some(viewer.login);
                guard.token_loaded = true;
                guard.is_refreshing = false;
                guard.last_refreshed_at = Some(Utc::now());
                guard.last_error = None;
                guard.pull_requests = items;
            }
            Err(error) => set_error(state, format!("{error:#}")),
        },
        Err(error) => set_error(state, format!("{error:#}")),
    }

    let _ = proxy.send_event(AppEvent::StateChanged);
}

async fn act_on_notification_thread(
    state: &Arc<Mutex<AppState>>,
    proxy: &EventLoopProxy<AppEvent>,
    notification_tracker: &mut NotificationTracker,
    token: Option<&str>,
    thread_id: &str,
    action: NotificationThreadAction,
) {
    let Some(token) = token else {
        set_error(
            state,
            format!("{} requires GitHub sign-in.", action.label()),
        );
        let _ = proxy.send_event(AppEvent::StateChanged);
        return;
    };

    {
        let mut guard = state.lock().expect("state lock");
        guard.last_error = None;
        guard.last_status = Some(format!("{} notification...", action.label()));
    }
    let _ = proxy.send_event(AppEvent::StateChanged);

    let client = GitHubClient::new(token);
    let result = match action {
        NotificationThreadAction::Done => client.mark_notification_thread_done(thread_id).await,
        NotificationThreadAction::Mute => client.mute_notification_thread(thread_id).await,
    };

    match result {
        Ok(()) => {
            refresh(state, proxy, notification_tracker, Some(token)).await;
            {
                let mut guard = state.lock().expect("state lock");
                guard.last_status = Some(format!("{} notification", action.label()));
            }
            let _ = proxy.send_event(AppEvent::StateChanged);
        }
        Err(error) => {
            set_error(state, format!("{error:#}"));
            let _ = proxy.send_event(AppEvent::StateChanged);
        }
    }
}

async fn check_for_updates(
    state: &Arc<Mutex<AppState>>,
    proxy: &EventLoopProxy<AppEvent>,
    show_status: bool,
) {
    let result = updater::check_latest_release(env!("CARGO_PKG_VERSION")).await;

    let mut guard = state.lock().expect("state lock");
    guard.is_checking_updates = false;
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
                guard.available_update = None;
                guard.update_status = Some(format!("Update check failed: {error:#}"));
            }
        }
    }

    let _ = proxy.send_event(AppEvent::StateChanged);
}

fn start_app_update(state: &Arc<Mutex<AppState>>, proxy: &EventLoopProxy<AppEvent>) {
    let update = state.lock().expect("state lock").available_update.clone();

    let Some(update) = update else {
        let mut guard = state.lock().expect("state lock");
        guard.available_update = None;
        guard.update_status = Some("There is no update ready to install.".to_string());
        let _ = proxy.send_event(AppEvent::StateChanged);
        return;
    };

    let (Some(download_url), Some(download_name), Some(checksum_url)) = (
        update.download_url,
        update.download_name,
        update.checksum_url,
    ) else {
        let mut guard = state.lock().expect("state lock");
        guard.available_update = None;
        guard.update_status =
            Some("This release does not include a verified app update.".to_string());
        let _ = proxy.send_event(AppEvent::StateChanged);
        return;
    };

    match updater::start_app_update(
        &download_url,
        &download_name,
        &checksum_url,
        &update.latest_version,
    ) {
        Ok(()) => {
            let mut guard = state.lock().expect("state lock");
            guard.available_update = None;
            guard.update_status =
                Some("Update started. Pullbell will restart when it is ready.".to_string());
        }
        Err(error) => {
            let mut guard = state.lock().expect("state lock");
            guard.available_update = None;
            guard.update_status = Some(format!("Update could not start: {error:#}"));
        }
    }

    let _ = proxy.send_event(AppEvent::StateChanged);
}

fn clear_error(state: &Arc<Mutex<AppState>>) {
    let mut guard = state.lock().expect("state lock");
    guard.last_error = None;
    guard.last_status = None;
}

fn set_error(state: &Arc<Mutex<AppState>>, error: String) {
    let mut guard = state.lock().expect("state lock");
    guard.is_refreshing = false;
    guard.pending_auth = None;
    guard.last_error = Some(error);
    guard.last_status = None;
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
            let client_id = DEFAULT_CLIENT_ID.trim();
            (!client_id.is_empty()).then(|| client_id.to_string())
        })
}
