use anyhow::{Context, Result};
use pullbell::state::AppState;
use std::sync::{Arc, Mutex};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const MENU_BAR_ICON_PNG: &[u8] = include_bytes!("../assets/pullbell-menu-bar-icon.png");

pub(super) fn update_tray(tray: &mut TrayIcon, state: &Arc<Mutex<AppState>>) -> Result<()> {
    let snapshot = state.lock().expect("state lock").clone();

    tray.set_title(Some(&snapshot.tray_title()));
    tray.set_tooltip(Some("Pullbell"))?;

    Ok(())
}

pub(super) fn build_tray() -> Result<TrayIcon> {
    TrayIconBuilder::new()
        .with_icon(build_icon()?)
        .with_icon_as_template(true)
        .with_title("PR")
        .with_tooltip("Pullbell")
        .build()
        .context("building macOS menu bar icon")
}

fn build_icon() -> Result<Icon> {
    let icon = image::load_from_memory_with_format(MENU_BAR_ICON_PNG, image::ImageFormat::Png)
        .context("decoding Pullbell menu bar icon")?
        .into_rgba8();
    let (width, height) = icon.dimensions();

    Icon::from_rgba(icon.into_raw(), width, height).context("building tray icon")
}
