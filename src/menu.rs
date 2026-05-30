use anyhow::{Context, Result};
use pullbell::state::AppState;
use std::sync::{Arc, Mutex};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub(super) fn update_tray(tray: &mut TrayIcon, state: &Arc<Mutex<AppState>>) -> Result<()> {
    let snapshot = state.lock().expect("state lock").clone();

    tray.set_title(Some(&snapshot.tray_title()));
    tray.set_tooltip(Some("Pullbell"))?;

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
