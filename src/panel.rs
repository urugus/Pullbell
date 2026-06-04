use super::AppEvent;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use pullbell::model::{PrKind, PullRequestItem};
use pullbell::state::AppState;
use pullbell::updater;
use std::collections::BTreeSet;
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event_loop::{EventLoop, EventLoopProxy};
#[cfg(target_os = "macos")]
use tao::platform::macos::WindowBuilderExtMacOS;
use tao::window::{Window, WindowBuilder, WindowId};
use tray_icon::Rect;
use wry::{WebView, WebViewBuilder};

const PANEL_WIDTH: f64 = 520.0;
const PANEL_HEIGHT: f64 = 680.0;
const MAX_ITEMS_PER_GROUP: usize = 8;

pub(super) struct Panel {
    window: Window,
    webview: WebView,
    visible: bool,
}

impl Panel {
    pub(super) fn new(
        event_loop: &EventLoop<AppEvent>,
        proxy: EventLoopProxy<AppEvent>,
        snapshot: &AppState,
    ) -> Result<Self> {
        let mut builder = WindowBuilder::new()
            .with_title("Pullbell")
            .with_inner_size(LogicalSize::new(PANEL_WIDTH, PANEL_HEIGHT))
            .with_resizable(false)
            .with_visible(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(true);

        #[cfg(target_os = "macos")]
        {
            builder = builder
                .with_title_hidden(true)
                .with_fullsize_content_view(true)
                .with_has_shadow(true);
        }

        let window = builder
            .build(event_loop)
            .context("building Pullbell panel")?;
        let html = html(snapshot);
        let webview = WebViewBuilder::new()
            .with_transparent(true)
            .with_html(html)
            .with_ipc_handler(move |request| {
                let _ = proxy.send_event(AppEvent::PanelCommand(request.body().clone()));
            })
            .build(&window)
            .context("building Pullbell web view")?;

        Ok(Self {
            window,
            webview,
            visible: false,
        })
    }

    pub(super) fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub(super) fn is_visible(&self) -> bool {
        self.visible
    }

    pub(super) fn show_near_or_default(&mut self, rect: Option<Rect>) {
        if let Some(rect) = rect {
            self.show_near(rect);
        } else {
            self.show_near_screen_edge();
        }
    }

    pub(super) fn hide(&mut self) {
        self.window.set_visible(false);
        self.visible = false;
    }

    pub(super) fn update(&self, snapshot: &AppState) -> Result<()> {
        let body = render_body(snapshot);
        let body_json = serde_json::to_string(&body)?;
        self.webview
            .evaluate_script(&format!("window.PullbellRender({body_json});"))
            .context("updating Pullbell panel")
    }

    pub(super) fn show_near(&mut self, rect: Rect) {
        let x = (rect.position.x + f64::from(rect.size.width) - PANEL_WIDTH + 16.0).max(8.0);
        let y = rect.position.y + f64::from(rect.size.height) + 8.0;

        self.window
            .set_outer_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        self.show();
    }

    fn show_near_screen_edge(&mut self) {
        let monitor = self
            .window
            .primary_monitor()
            .or_else(|| self.window.current_monitor());
        let (x, y) = if let Some(monitor) = monitor {
            let position = monitor.position();
            let size = monitor.size();
            (
                f64::from(position.x) + f64::from(size.width) - PANEL_WIDTH - 16.0,
                f64::from(position.y) + 8.0,
            )
        } else {
            (8.0, 8.0)
        };

        self.window.set_outer_position(PhysicalPosition::new(
            x.max(8.0).round() as i32,
            y.max(8.0).round() as i32,
        ));
        self.show();
    }

    fn show(&mut self) {
        activate_application();
        self.window.set_visible(true);
        self.window.set_focus();
        self.visible = true;
    }
}

#[cfg(target_os = "macos")]
fn activate_application() {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::MainThreadMarker;

    // SAFETY: panel visibility is managed from tao's main event-loop thread.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}

#[cfg(not(target_os = "macos"))]
fn activate_application() {}

fn html(snapshot: &AppState) -> String {
    format!(
        r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
* {{ box-sizing: border-box; }}
:root {{
  color-scheme: dark;
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
  background: transparent;
}}
html, body {{
  margin: 0;
  width: 100%;
  min-height: 100%;
  overflow: hidden;
  background: transparent;
}}
body {{
  padding: 10px;
  color: #f4f5f6;
  font-size: 13px;
  line-height: 1.35;
  letter-spacing: 0;
}}
button {{
  font: inherit;
  letter-spacing: 0;
}}
.panel {{
  position: relative;
  width: 500px;
  height: 660px;
  overflow: hidden;
  border: 1px solid rgba(255,255,255,.12);
  border-radius: 18px;
  background: #1d1f23;
  box-shadow: 0 22px 70px rgba(0,0,0,.48), 0 2px 12px rgba(0,0,0,.34);
}}
.shell {{
  display: grid;
  grid-template-rows: auto 1fr auto;
  height: 100%;
}}
.topbar {{
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 16px 16px 10px;
  border-bottom: 1px solid rgba(255,255,255,.07);
}}
.brand {{
  display: flex;
  align-items: center;
  min-width: 0;
  gap: 10px;
}}
.mark {{
  width: 25px;
  height: 25px;
  border-radius: 8px;
  background: linear-gradient(145deg, #f7f7f8 0%, #a9adb7 100%);
  color: #16181c;
  display: grid;
  place-items: center;
  font-weight: 800;
}}
.brand-text {{
  min-width: 0;
}}
.name {{
  font-size: 15px;
  font-weight: 650;
}}
.subtle {{
  color: #9ba0aa;
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}}
.counter {{
  min-width: 34px;
  height: 24px;
  border-radius: 999px;
  display: grid;
  place-items: center;
  background: #f7f8fb;
  color: #17191d;
  font-size: 12px;
  font-weight: 700;
}}
.content {{
  overflow: auto;
  padding: 8px 8px 12px;
}}
.view {{
  display: none;
}}
.view.active {{
  display: block;
}}
.filters {{
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 6px;
  padding: 8px 6px 4px;
}}
.filter {{
  min-width: 0;
  height: 28px;
  border: 1px solid rgba(255,255,255,.08);
  border-radius: 8px;
  padding: 0 8px;
  color: #d9dce2;
  background: #252930;
  font: inherit;
  font-size: 11px;
}}
.filter:focus {{
  outline: 1px solid rgba(255,255,255,.20);
  outline-offset: 0;
}}
.section {{
  padding: 8px 6px 2px;
}}
.heading {{
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 5px 8px 6px;
  color: #7f8490;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
}}
.heading span:last-child {{
  color: #606672;
}}
.row {{
  width: 100%;
  min-height: 62px;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 28px 28px auto;
  gap: 8px;
  align-items: center;
  padding: 9px 10px 9px 0;
  border: 0;
  border-radius: 10px;
  color: inherit;
  background: transparent;
  text-align: left;
  cursor: default;
}}
.row:hover,
.row:focus-within {{
  background: #2c3036;
}}
.row:focus {{
  outline: none;
}}
.row.selected {{
  background: #313640;
  box-shadow: inset 0 0 0 1px rgba(255,255,255,.10);
}}
.row-open {{
  min-width: 0;
  display: grid;
  grid-template-columns: 34px minmax(0, 1fr);
  gap: 10px;
  align-items: center;
  padding: 0 0 0 10px;
  border: 0;
  color: inherit;
  background: transparent;
  text-align: left;
}}
.row-open:focus {{
  outline: none;
}}
.row-open:active {{
  color: inherit;
}}
.row-main {{
  min-width: 0;
}}
.row-action {{
  width: 28px;
  height: 28px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 8px;
  color: #d6d9df;
  background: transparent;
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
}}
.row.selected .row-action,
.row:focus-within .row-action {{
  opacity: 1;
  visibility: visible;
  pointer-events: auto;
}}
.row-action:hover {{
  color: #ffffff;
  background: #3a404a;
}}
.row-action:focus {{
  outline: 1px solid rgba(255,255,255,.22);
  outline-offset: 0;
}}
.row-action svg {{
  width: 15px;
  height: 15px;
  stroke: currentColor;
}}
.badge {{
  width: 28px;
  height: 28px;
  display: grid;
  place-items: center;
  color: #9ca1ac;
}}
.badge svg {{
  width: 20px;
  height: 20px;
  stroke: currentColor;
}}
.badge.review {{ color: #9b87ff; }}
.badge.notify {{ color: #3fb6f2; }}
.badge.authored {{ color: #8c949f; }}
.badge.ci {{ color: #ff6b6b; }}
.badge.mention {{ color: #49c6f5; }}
.badge.security {{ color: #ffb84d; }}
.badge.state {{ color: #75d08a; }}
.badge.assign {{ color: #f2c94c; }}
.badge.muted {{ color: #707681; }}
.row.done .badge {{ color: #676d77; }}
.main {{
  min-width: 0;
}}
.meta {{
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  color: #9ca1ac;
  font-size: 11px;
}}
.repo {{
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}}
.dot {{
  width: 3px;
  height: 3px;
  border-radius: 999px;
  background: #656b76;
  flex: 0 0 auto;
}}
.title {{
  margin-top: 3px;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #f0f1f3;
  font-size: 13px;
  font-weight: 540;
}}
.age {{
  align-self: start;
  padding-top: 2px;
  color: #7a808b;
  font-size: 11px;
  white-space: nowrap;
}}
.empty {{
  margin: 2px 8px 8px;
  padding: 14px 12px;
  border-radius: 10px;
  color: #8d939f;
  background: rgba(255,255,255,.035);
}}
.pinned {{
  margin: 2px 8px 8px;
  padding: 11px 12px;
  border-radius: 10px;
  background: #282c33;
  border: 1px solid rgba(255,255,255,.07);
}}
.pinned-title {{
  font-weight: 650;
  color: #f2f3f5;
}}
.pinned-body {{
  margin-top: 3px;
  color: #a8adb8;
  font-size: 12px;
}}
.code {{
  margin-top: 8px;
  width: max-content;
  max-width: 100%;
  padding: 5px 8px;
  border-radius: 7px;
  background: #15171b;
  color: #ffffff;
  font-weight: 800;
  letter-spacing: .08em;
}}
.footer {{
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px 12px;
  border-top: 1px solid rgba(255,255,255,.08);
  background: rgba(22,24,28,.84);
}}
.tool {{
  height: 30px;
  min-width: 30px;
  border: 0;
  border-radius: 9px;
  display: grid;
  place-items: center;
  padding: 0 9px;
  color: #d7dae0;
  background: transparent;
}}
.tool:hover {{
  background: #30343b;
  color: #ffffff;
}}
.tool.icon {{
  width: 30px;
  min-width: 30px;
  padding: 0;
}}
.tool svg {{
  width: 17px;
  height: 17px;
  stroke: currentColor;
}}
.tool.primary {{
  min-width: 72px;
  background: #f4f5f7;
  color: #17191d;
  font-weight: 700;
}}
.spacer {{ flex: 1; }}
.settings {{
  padding: 0;
  background: #202329;
}}
.settings-head {{
  display: grid;
  grid-template-columns: 32px minmax(0, 1fr);
  gap: 10px;
  align-items: center;
  padding: 14px 14px 12px;
  border-bottom: 1px solid rgba(255,255,255,.08);
}}
.settings-title {{
  min-width: 0;
  color: #f3f4f6;
  font-size: 18px;
  font-weight: 700;
}}
.settings-subtitle {{
  margin-top: 2px;
  color: #9298a3;
  font-size: 11px;
}}
.settings-body {{
  padding: 10px 14px 16px;
}}
.settings-section {{
  padding: 12px 0 14px;
  border-bottom: 1px solid rgba(255,255,255,.07);
}}
.settings-section:last-child {{
  border-bottom: 0;
}}
.settings-label {{
  color: #7f8490;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
}}
.setting-row {{
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 14px;
  align-items: center;
  min-height: 46px;
  padding: 8px 0;
}}
.setting-name {{
  color: #f0f1f3;
  font-size: 13px;
  font-weight: 620;
}}
.setting-note {{
  margin-top: 3px;
  color: #969ca7;
  font-size: 11px;
}}
.switch {{
  position: relative;
  width: 42px;
  height: 24px;
  border: 0;
  border-radius: 999px;
  background: #3a404a;
  transition: background .14s ease-out;
}}
.switch::after {{
  content: "";
  position: absolute;
  top: 3px;
  left: 3px;
  width: 18px;
  height: 18px;
  border-radius: 999px;
  background: #cfd3da;
  box-shadow: 0 1px 3px rgba(0,0,0,.35);
  transition: transform .14s ease-out, background .14s ease-out;
}}
.switch.on {{
  background: #3f8cff;
}}
.switch.on::after {{
  background: #ffffff;
  transform: translateX(18px);
}}
.settings-filters {{
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 7px;
  padding-top: 10px;
}}
.settings-filters .filter {{
  width: 100%;
}}
.settings-actions {{
  display: flex;
  flex-wrap: wrap;
  gap: 7px;
  padding-top: 10px;
}}
.settings-actions .tool {{
  border: 1px solid rgba(255,255,255,.08);
  background: #2b3038;
}}
.settings-actions .tool:hover {{
  background: #343a44;
}}
.settings-actions .tool:disabled {{
  color: #747b86;
  background: #252930;
}}
.repo-search {{
  width: 100%;
  height: 30px;
  margin-top: 10px;
  border: 1px solid rgba(255,255,255,.08);
  border-radius: 8px;
  padding: 0 9px;
  color: #d9dce2;
  background: #252930;
  font: inherit;
  font-size: 12px;
}}
.repo-settings {{
  display: grid;
  gap: 2px;
  padding-top: 8px;
}}
.repo-setting {{
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 12px;
  align-items: center;
  min-height: 38px;
}}
.repo-setting-name {{
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #e7e9ee;
  font-size: 12px;
}}
.repo-setting-note {{
  margin-top: 2px;
  color: #858b96;
  font-size: 11px;
}}
.chips {{
  display: flex;
  flex-wrap: wrap;
  gap: 7px;
  padding-top: 10px;
}}
.chip {{
  max-width: 100%;
  min-height: 24px;
  display: inline-flex;
  align-items: center;
  border-radius: 999px;
  padding: 4px 8px;
  color: #c9ced7;
  background: #2b3038;
  font-size: 11px;
}}
.chip.empty {{
  color: #8b929e;
}}
.preview {{
  position: absolute;
  inset: 58px 10px 52px;
  display: none;
  grid-template-rows: auto 1fr;
  overflow: hidden;
  border: 1px solid rgba(255,255,255,.12);
  border-radius: 14px;
  background: #22262d;
  box-shadow: 0 18px 54px rgba(0,0,0,.46);
  z-index: 3;
}}
.preview.open {{
  display: grid;
}}
.preview-head {{
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 12px;
  padding: 14px 14px 10px;
  border-bottom: 1px solid rgba(255,255,255,.08);
}}
.preview-title {{
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #f3f4f6;
  font-weight: 650;
}}
.preview-meta {{
  margin-top: 4px;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #969ca7;
  font-size: 11px;
}}
.preview-close {{
  width: 28px;
  height: 28px;
  border: 0;
  border-radius: 8px;
  color: #d6d9df;
  background: transparent;
}}
.preview-close:hover {{
  background: #323741;
}}
.preview-body {{
  overflow: auto;
  padding: 13px 14px 16px;
  color: #d7dae0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-size: 12px;
  line-height: 1.45;
}}
</style>
</head>
<body>
<div class="panel"><div id="app" class="shell">{}</div></div>
<script>
window.send = function(message) {{ window.ipc.postMessage(message); }};
(function() {{
  const app = document.getElementById("app");
  let selectedIndex = 0;
  let filters = {{ repo: "", reason: "", author: "" }};
  let previewOpen = false;
  let currentView = "notifications";
  let groupByRepository = false;
  let pendingSelectionDoneCmd = null;

  function selectableRows() {{
    return Array.from(document.querySelectorAll("[data-selectable='true']"));
  }}

  function visibleRows() {{
    return selectableRows().filter(function(row) {{ return !row.hidden; }});
  }}

  function visibleTodoRows() {{
    return visibleRows().filter(function(row) {{
      return row.dataset.todoRow === "true";
    }});
  }}

  function clamp(index, count) {{
    if (count === 0) return 0;
    return Math.max(0, Math.min(index, count - 1));
  }}

  window.PullbellSelect = function(index, shouldFocus) {{
    const rows = visibleRows();
    selectedIndex = clamp(index, rows.length);

    selectableRows().forEach(function(row) {{
      const selected = rows[selectedIndex] === row;
      row.classList.toggle("selected", selected);
      row.tabIndex = selected ? 0 : -1;
      row.setAttribute("aria-selected", selected ? "true" : "false");
    }});

    if (rows.length > 0 && shouldFocus) {{
      rows[selectedIndex].focus({{ preventScroll: true }});
      rows[selectedIndex].scrollIntoView({{ block: "nearest" }});
    }}

    if (previewOpen) window.PullbellShowPreview();
  }};

  window.PullbellSelectElement = function(element) {{
    const index = visibleRows().indexOf(element);
    if (index >= 0) window.PullbellSelect(index, false);
  }};

  window.PullbellActivateSelected = function() {{
    const row = visibleRows()[selectedIndex];
    if (row && row.dataset.cmd) window.send(row.dataset.cmd);
  }};

  window.PullbellCopySelected = function() {{
    const row = visibleRows()[selectedIndex];
    if (row && row.dataset.copyCmd) window.send(row.dataset.copyCmd);
  }};

  function rememberSelectionAfterDone(row) {{
    if (!row || row.classList.contains("done")) return;

    const rows = visibleTodoRows();
    const index = rows.indexOf(row);
    if (index < 0) return;

    const nextRow = rows[index + 1] || rows[index - 1];
    pendingSelectionDoneCmd = nextRow ? nextRow.dataset.doneCmd : null;
  }}

  function restorePendingSelection() {{
    if (!pendingSelectionDoneCmd) return false;

    const command = pendingSelectionDoneCmd;
    pendingSelectionDoneCmd = null;

    const rows = visibleRows();
    const index = rows.findIndex(function(row) {{
      return row.dataset.doneCmd === command;
    }});
    if (index < 0) return false;

    window.PullbellSelect(index, currentView === "notifications");
    return true;
  }}

  window.PullbellActOnSelected = function(action) {{
    const row = visibleRows()[selectedIndex];
    if (!row) return;

    if (action === "done") rememberSelectionAfterDone(row);

    const command =
      action === "done" ? row.dataset.doneCmd :
      action === "undo" ? row.dataset.undoCmd :
      action === "mute-repo" ? row.dataset.muteRepoCmd :
      row.dataset.muteCmd;
    window.send(command || "missing-action:" + action);
  }};

  window.PullbellShowPreview = function() {{
    const row = visibleRows()[selectedIndex];
    const preview = document.getElementById("preview");
    if (!row || !preview) return;

    document.getElementById("preview-title").textContent = row.dataset.previewTitle || "";
    document.getElementById("preview-meta").textContent = row.dataset.previewMeta || "";
    document.getElementById("preview-body").textContent = row.dataset.previewBody || "No preview text available.";
    preview.classList.add("open");
    previewOpen = true;
  }};

  window.PullbellHidePreview = function() {{
    const preview = document.getElementById("preview");
    if (preview) preview.classList.remove("open");
    previewOpen = false;
  }};

  window.PullbellTogglePreview = function() {{
    if (previewOpen) {{
      window.PullbellHidePreview();
    }} else {{
      window.PullbellShowPreview();
    }}
  }};

  window.PullbellSetView = function(view) {{
    currentView = view === "settings" ? "settings" : "notifications";
    document.querySelectorAll("[data-panel-view]").forEach(function(element) {{
      element.classList.toggle("active", element.dataset.panelView === currentView);
    }});
    const settingsButton = document.querySelector("[data-open-settings]");
    if (settingsButton) {{
      settingsButton.setAttribute("aria-expanded", currentView === "settings" ? "true" : "false");
    }}
    if (currentView === "settings") window.PullbellHidePreview();
  }};

  window.PullbellShowSettings = function() {{
    window.PullbellSetView("settings");
  }};

  window.PullbellShowNotifications = function() {{
    window.PullbellSetView("notifications");
  }};

  window.PullbellSyncGroupByRepository = function() {{
    document.querySelectorAll("[data-group-by-repository]").forEach(function(control) {{
      control.classList.toggle("on", groupByRepository);
      control.setAttribute("aria-pressed", groupByRepository ? "true" : "false");
    }});
  }};

  window.PullbellToggleGroupByRepository = function() {{
    groupByRepository = !groupByRepository;
    window.PullbellSyncGroupByRepository();
  }};

  function syncFilterControls() {{
    Object.keys(filters).forEach(function(name) {{
      const controls = Array.from(document.querySelectorAll("[data-filter='" + name + "']"));
      if (controls.length === 0) return;
      const hasValue = controls.some(function(control) {{
        return Array.from(control.options).some(function(option) {{
          return option.value === filters[name];
        }});
      }});
      if (!hasValue) filters[name] = "";
      controls.forEach(function(control) {{
        control.value = filters[name];
      }});
    }});
  }}

  window.PullbellApplyFilters = function() {{
    selectableRows().forEach(function(row) {{
      const visible =
        (!filters.repo || row.dataset.repo === filters.repo) &&
        (!filters.reason || row.dataset.reason === filters.reason) &&
        (!filters.author || row.dataset.author === filters.author);
      row.hidden = !visible;
    }});
    window.PullbellSelect(selectedIndex, false);
  }};

  window.PullbellRender = function(html) {{
    app.innerHTML = html;
    syncFilterControls();
    window.PullbellApplyFilters();
    window.PullbellSetView(currentView);
    window.PullbellSyncGroupByRepository();
    restorePendingSelection();
    if (previewOpen) window.PullbellShowPreview();
  }};

  document.addEventListener("change", function(event) {{
    const name = event.target.dataset && event.target.dataset.filter;
    if (!name) return;
    filters[name] = event.target.value;
    selectedIndex = 0;
    syncFilterControls();
    window.PullbellApplyFilters();
  }});

  document.addEventListener("input", function(event) {{
    if (!event.target.matches("[data-repository-search]")) return;
    const query = event.target.value.trim().toLowerCase();
    document.querySelectorAll("[data-repository-setting]").forEach(function(row) {{
      row.hidden = query && !row.dataset.repositorySetting.toLowerCase().includes(query);
    }});
  }});

  document.addEventListener("keydown", function(event) {{
    if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.target && /^(SELECT|INPUT|TEXTAREA)$/.test(event.target.tagName)) return;
    const key = event.key.toLowerCase();
    if (key === "escape") {{
      event.preventDefault();
      if (currentView === "settings") {{
        window.PullbellShowNotifications();
        return;
      }}
      if (previewOpen) {{
        window.PullbellHidePreview();
        return;
      }}
      window.send("hide");
      return;
    }}
    if (event.target && event.target.tagName === "BUTTON" && event.target.dataset.rowShortcut !== "true") return;
    if (currentView !== "notifications") return;

    if (key === "j" || event.key === "ArrowDown") {{
      event.preventDefault();
      window.PullbellSelect(selectedIndex + 1, true);
    }} else if (key === "k" || event.key === "ArrowUp") {{
      event.preventDefault();
      window.PullbellSelect(selectedIndex - 1, true);
    }} else if (key === "enter" || key === "o") {{
      event.preventDefault();
      window.PullbellActivateSelected();
    }} else if (key === "c") {{
      event.preventDefault();
      window.PullbellCopySelected();
    }} else if (event.key === " ") {{
      event.preventDefault();
      window.PullbellTogglePreview();
    }} else if (key === "d") {{
      event.preventDefault();
      window.PullbellActOnSelected("done");
    }} else if (key === "u") {{
      event.preventDefault();
      window.PullbellActOnSelected("undo");
    }} else if (key === "m") {{
      event.preventDefault();
      window.PullbellActOnSelected("mute");
    }} else if (key === "r") {{
      event.preventDefault();
      window.send("refresh");
    }} else if (key === "i") {{
      event.preventDefault();
      window.send("inbox");
    }} else if (key === "q") {{
      event.preventDefault();
      window.send("quit");
    }}
  }});

  syncFilterControls();
  window.PullbellApplyFilters();
  window.PullbellSyncGroupByRepository();
}})();
</script>
</body>
</html>"#,
        render_body(snapshot)
    )
}

fn render_body(snapshot: &AppState) -> String {
    let mut html = String::new();
    let (todo_count, done_count) = snapshot.todo_done_counts();

    html.push_str(&render_topbar(snapshot, todo_count));
    html.push_str(
        r#"<main id="notifications-view" class="content view active" data-panel-view="notifications">"#,
    );
    html.push_str(&render_filters(snapshot));
    html.push_str(&render_pinned(snapshot));
    html.push_str(&render_section(
        "To do",
        todo_count,
        snapshot.pull_requests.iter().filter(|item| item.is_todo()),
        "All caught up",
    ));
    html.push_str(&render_section(
        "Done",
        done_count,
        snapshot.pull_requests.iter().filter(|item| !item.is_todo()),
        "No open PRs being tracked",
    ));
    html.push_str("</main>");
    html.push_str(&render_settings(snapshot));
    html.push_str(&render_footer(snapshot));
    html.push_str(&render_preview());
    html
}

fn render_topbar(snapshot: &AppState, todo_count: usize) -> String {
    let status = if let Some(login) = &snapshot.signed_in_as {
        format!("Signed in as {}", escape_html(login))
    } else if snapshot.token_loaded {
        "Signed in".to_string()
    } else {
        "Not signed in".to_string()
    };

    format!(
        r#"<header class="topbar"><div class="brand"><div class="mark">P</div><div class="brand-text"><div class="name">Pullbell</div><div class="subtle">{} · v{}</div></div></div><div class="counter">{}</div></header>"#,
        status,
        env!("CARGO_PKG_VERSION"),
        todo_count
    )
}

fn render_filters(snapshot: &AppState) -> String {
    format!(
        r#"<div class="filters">{}</div>"#,
        render_filter_controls(snapshot)
    )
}

fn render_filter_controls(snapshot: &AppState) -> String {
    let repos = sorted_values(snapshot.pull_requests.iter().map(|item| item.repo.clone()));
    let reasons = sorted_values(snapshot.pull_requests.iter().map(reason_label));
    let authors = sorted_values(
        snapshot
            .pull_requests
            .iter()
            .filter_map(|item| item.author.clone()),
    );

    format!(
        r#"{}{}{}"#,
        render_filter("repo", "Repository", &repos),
        render_filter("reason", "Reason", &reasons),
        render_filter("author", "User", &authors)
    )
}

fn render_settings(snapshot: &AppState) -> String {
    let mut html = format!(
        r#"<main id="settings-view" class="content view settings" data-panel-view="settings"><div class="settings-head"><button class="tool icon" title="Back to notifications" aria-label="Back to notifications" onclick="window.PullbellShowNotifications()">{}</button><div><div class="settings-title">Settings</div><div class="settings-subtitle">Manage Pullbell controls and app actions.</div></div></div><div class="settings-body">"#,
        back_icon()
    );

    html.push_str(&render_account_settings(snapshot));
    html.push_str(&render_update_settings(snapshot));
    html.push_str(&render_repository_settings(snapshot));
    html.push_str(
        r#"<section class="settings-section"><div class="settings-label">View</div><div class="setting-row"><div><div class="setting-name">Group by Repository</div><div class="setting-note">Preview control only. The notification list keeps the current order.</div></div><button class="switch" type="button" data-group-by-repository aria-label="Group by Repository" aria-pressed="false" onclick="window.PullbellToggleGroupByRepository()"></button></div></section>"#,
    );
    html.push_str(&format!(
        r#"<section class="settings-section"><div class="settings-label">Focus filters</div><div class="setting-note">These controls mirror the notification list filters.</div><div class="settings-filters">{}</div></section>"#,
        render_filter_controls(snapshot)
    ));
    html.push_str(&format!(
        r#"<section class="settings-section"><div class="settings-label">Muted</div><div class="setting-note">Available repositories, reasons, and users you can filter out for this session.</div>{}</section>"#,
        render_muted_chips(snapshot)
    ));
    html.push_str(&render_app_settings());
    html.push_str("</div></main>");
    html
}

fn render_account_settings(snapshot: &AppState) -> String {
    let account_note = if let Some(login) = &snapshot.signed_in_as {
        format!("Signed in as {}", escape_html(login))
    } else if snapshot.token_loaded {
        "Signed in".to_string()
    } else if snapshot.pending_auth.is_some() {
        "GitHub sign-in is waiting for device approval.".to_string()
    } else {
        "Not signed in".to_string()
    };

    let mut html = format!(
        r#"<section class="settings-section"><div class="settings-label">Account</div><div class="setting-row"><div><div class="setting-name">GitHub</div><div class="setting-note">{}</div></div></div><div class="settings-actions">"#,
        account_note
    );

    if !snapshot.token_loaded && snapshot.pending_auth.is_none() {
        html.push_str(
            r#"<button class="tool primary" onclick="send('signin')">Sign in with GitHub</button>"#,
        );
    }

    if let Some(auth) = &snapshot.pending_auth {
        html.push_str(&format!(
            r#"<button class="tool primary" data-cmd="open:{}" onclick="send(this.dataset.cmd)">Open GitHub</button><button class="tool" onclick="send('copy-signin-code')">Copy code</button>"#,
            escape_attr(&auth.verification_uri)
        ));
    }

    if snapshot.token_loaded {
        html.push_str(r#"<button class="tool" onclick="send('signout')">Sign out</button>"#);
    }

    html.push_str("</div></section>");
    html
}

fn render_update_settings(snapshot: &AppState) -> String {
    let note = if snapshot.is_installing_update {
        "Updating with Homebrew. A restart prompt will appear when it is ready.".to_string()
    } else if let Some(update) = &snapshot.available_update {
        format!("Update available: v{}", escape_html(&update.latest_version))
    } else if snapshot.is_checking_updates {
        "Checking the latest release.".to_string()
    } else if let Some(checked_at) = snapshot.last_update_checked_at {
        format!("Last checked at {}", checked_at.format("%H:%M:%S"))
    } else {
        "Check GitHub Releases for a newer version.".to_string()
    };

    let mut html = format!(
        r#"<section class="settings-section"><div class="settings-label">Updates</div><div class="setting-row"><div><div class="setting-name">Pullbell version {}</div><div class="setting-note">{}</div></div></div><div class="settings-actions">"#,
        env!("CARGO_PKG_VERSION"),
        note
    );

    if let Some(update) = &snapshot.available_update {
        if snapshot.is_installing_update {
            html.push_str(r#"<button class="tool primary" disabled>Updating...</button>"#);
        } else {
            html.push_str(r#"<button class="tool primary" onclick="send('install-update')">Update with Homebrew</button>"#);
        }
        html.push_str(&format!(
            r#"<button class="tool" data-cmd="open:{}" onclick="send(this.dataset.cmd)">Open release</button>"#,
            escape_attr(&update.release_url)
        ));
    } else {
        if snapshot.is_checking_updates {
            html.push_str(r#"<button class="tool" disabled>Checking...</button>"#);
        } else {
            html.push_str(r#"<button class="tool" onclick="send('check-updates')">Check for updates</button>"#);
        }
        html.push_str(&format!(
            r#"<button class="tool" data-cmd="open:{}" onclick="send(this.dataset.cmd)">Open releases</button>"#,
            escape_attr(updater::RELEASES_URL)
        ));
    }

    html.push_str("</div></section>");
    html
}

fn render_repository_settings(snapshot: &AppState) -> String {
    let repositories = snapshot
        .settings
        .known_repositories
        .union(&snapshot.settings.muted_repositories)
        .cloned()
        .collect::<Vec<_>>();

    let muted_count = snapshot.settings.muted_repositories.len();
    let mut html = format!(
        r#"<section class="settings-section"><div class="settings-label">Repositories</div><div class="setting-row"><div><div class="setting-name">Muted repositories</div><div class="setting-note">Muted repositories are hidden from Pullbell lists and desktop notifications. GitHub Watch settings are not changed.</div></div><div class="counter">{}</div></div>"#,
        muted_count
    );

    if repositories.is_empty() {
        html.push_str(r#"<div class="empty">No repositories seen yet</div>"#);
    } else {
        html.push_str(
            r#"<input class="repo-search" type="search" placeholder="Search repositories" aria-label="Search repositories" data-repository-search>"#,
        );
        html.push_str(r#"<div class="repo-settings">"#);
        for repo in repositories {
            let muted = snapshot.settings.muted_repositories.contains(&repo);
            let command = if muted {
                format!("unmute-repo:{repo}")
            } else {
                format!("mute-repo:{repo}")
            };
            let action_label = if muted { "Unmute" } else { "Mute" };
            let note = if muted {
                "Muted in Pullbell"
            } else {
                "Tracked by Pullbell"
            };
            html.push_str(&format!(
                r#"<div class="repo-setting" data-repository-setting="{}"><div><div class="repo-setting-name">{}</div><div class="repo-setting-note">{}</div></div><button class="switch{}" type="button" aria-label="{} {} in Pullbell" aria-pressed="{}" data-cmd="{}" onclick="send(this.dataset.cmd)"></button></div>"#,
                escape_attr(&repo),
                escape_html(&repo),
                note,
                if muted { " on" } else { "" },
                action_label,
                escape_attr(&repo),
                if muted { "true" } else { "false" },
                escape_attr(&command)
            ));
        }
        html.push_str("</div>");
    }

    html.push_str("</section>");
    html
}

fn render_app_settings() -> String {
    r#"<section class="settings-section"><div class="settings-label">App</div><div class="setting-row"><div><div class="setting-name">Pullbell</div><div class="setting-note">Close the menu bar app.</div></div></div><div class="settings-actions"><button class="tool" onclick="send('quit')">Quit Pullbell</button></div></section>"#.to_string()
}

fn render_muted_chips(snapshot: &AppState) -> String {
    let values = sorted_values(
        snapshot
            .pull_requests
            .iter()
            .flat_map(|item| {
                [
                    Some(item.repo.clone()),
                    Some(reason_label(item)),
                    item.author.clone(),
                ]
            })
            .flatten(),
    );

    if values.is_empty() {
        return r#"<div class="chips"><span class="chip empty">No mute targets available</span></div>"#
            .to_string();
    }

    let mut html = String::from(r#"<div class="chips">"#);
    for value in values.iter().take(8) {
        html.push_str(&format!(
            r#"<span class="chip">{}</span>"#,
            escape_html(short_filter_value(value))
        ));
    }

    if values.len() > 8 {
        html.push_str(&format!(
            r#"<span class="chip">+{} more</span>"#,
            values.len() - 8
        ));
    }

    html.push_str("</div>");
    html
}

fn render_filter(name: &str, label: &str, values: &[String]) -> String {
    let mut html = format!(
        r#"<select class="filter" data-filter="{}" aria-label="{}"><option value="">{}</option>"#,
        escape_attr(name),
        escape_attr(label),
        escape_html(label)
    );

    for value in values {
        html.push_str(&format!(
            r#"<option value="{}">{}</option>"#,
            escape_attr(value),
            escape_html(short_filter_value(value))
        ));
    }

    html.push_str("</select>");
    html
}

fn sorted_values(values: impl Iterator<Item = String>) -> Vec<String> {
    values
        .filter(|value| !value.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn render_pinned(snapshot: &AppState) -> String {
    let mut html = String::from(
        r#"<section class="section"><div class="heading"><span>Pinned</span><span></span></div>"#,
    );

    if let Some(auth) = &snapshot.pending_auth {
        html.push_str(&format!(
            r#"<div class="pinned"><div class="pinned-title">GitHub sign-in is waiting</div><div class="pinned-body">Enter this code on GitHub. It was copied to the clipboard.</div><div class="code">{}</div></div>"#,
            escape_html(&auth.user_code)
        ));
    } else if snapshot.is_installing_update {
        html.push_str(
            r#"<div class="pinned"><div class="pinned-title">Updating Pullbell</div><div class="pinned-body">Homebrew is updating the app. A restart prompt will appear when it is ready.</div></div>"#,
        );
    } else if let Some(update) = &snapshot.available_update {
        html.push_str(&format!(
            r#"<div class="pinned"><div class="pinned-title">Update available: v{}</div><div class="pinned-body">A newer Pullbell release is ready.</div></div>"#,
            escape_html(&update.latest_version)
        ));
    } else if let Some(status) = &snapshot.update_status {
        html.push_str(&format!(
            r#"<div class="pinned"><div class="pinned-title">Update status</div><div class="pinned-body">{}</div></div>"#,
            escape_html(status)
        ));
    } else if let Some(status) = &snapshot.last_status {
        html.push_str(&format!(
            r#"<div class="pinned"><div class="pinned-title">Status</div><div class="pinned-body">{}</div></div>"#,
            escape_html(status)
        ));
    } else if snapshot.is_checking_updates {
        html.push_str(
            r#"<div class="pinned"><div class="pinned-title">Checking for updates</div><div class="pinned-body">Pullbell is checking the latest release.</div></div>"#,
        );
    } else if let Some(error) = &snapshot.last_error {
        html.push_str(&format!(
            r#"<div class="pinned"><div class="pinned-title">Needs attention</div><div class="pinned-body">{}</div></div>"#,
            escape_html(error)
        ));
    } else if let Some(refreshed_at) = snapshot.last_refreshed_at {
        html.push_str(&format!(
            r#"<div class="pinned"><div class="pinned-title">Ready</div><div class="pinned-body">Last refreshed at {}</div></div>"#,
            refreshed_at.format("%H:%M:%S")
        ));
    } else if snapshot.token_loaded {
        html.push_str(
            r#"<div class="pinned"><div class="pinned-title">Ready</div><div class="pinned-body">Pullbell is watching your GitHub pull requests.</div></div>"#,
        );
    } else {
        html.push_str(
            r#"<div class="pinned"><div class="pinned-title">Sign in to start</div><div class="pinned-body">Connect GitHub to watch pull request activity.</div></div>"#,
        );
    }

    html.push_str("</section>");
    html
}

fn render_section<'a>(
    label: &str,
    count: usize,
    items: impl Iterator<Item = &'a PullRequestItem>,
    empty: &str,
) -> String {
    let mut html = format!(
        r#"<section class="section"><div class="heading"><span>{}</span><span>{}</span></div>"#,
        escape_html(label),
        count
    );
    let items: Vec<_> = items.take(MAX_ITEMS_PER_GROUP + 1).collect();

    if items.is_empty() {
        html.push_str(&format!(
            r#"<div class="empty">{}</div>"#,
            escape_html(empty)
        ));
    } else {
        let now = Utc::now();
        for item in items.iter().take(MAX_ITEMS_PER_GROUP) {
            html.push_str(&render_item(item, now));
        }

        if count > MAX_ITEMS_PER_GROUP {
            html.push_str(&format!(
                r#"<div class="empty">...and {} more</div>"#,
                count - MAX_ITEMS_PER_GROUP
            ));
        }
    }

    html.push_str("</section>");
    html
}

fn render_item(item: &PullRequestItem, now: DateTime<Utc>) -> String {
    let label = item_kind_label(item);
    let badge_class = item_icon_class(item);
    let badge_icon = item_icon(item);
    let row_class = if item.locally_done { "row done" } else { "row" };
    let todo_attr = if item.is_todo() {
        r#" data-todo-row="true""#
    } else {
        ""
    };
    let repo = short_repo_name(&item.repo);
    let age = item
        .updated_at
        .map(|updated_at| relative_age(updated_at, now))
        .unwrap_or_else(|| "unknown".to_string());
    let command = format!("open:{}", item.url);
    let done_attr = format!(r#" data-done-cmd="done-pr:{}""#, escape_attr(&item.id));
    let undo_attr = if item.locally_done {
        format!(r#" data-undo-cmd="undo-pr:{}""#, escape_attr(&item.id))
    } else {
        String::new()
    };
    let mute_attr = notification_action_attr("data-mute-cmd", "mute", item);
    let mute_repo_command = format!("mute-repo:{}", item.repo);
    let reason = reason_label(item);
    let author = item.author.as_deref().unwrap_or("");
    let preview_meta = preview_meta(item, &age, &reason);
    let preview_body = item
        .preview
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("No preview text available.");
    let copy_command = format!("copy-url:{}", item.url);

    format!(
        r#"<div class="{}" data-selectable="true" data-cmd="{}" data-copy-cmd="{}" data-mute-repo-cmd="{}" data-repo="{}" data-reason="{}" data-author="{}" data-preview-title="{}" data-preview-meta="{}" data-preview-body="{}"{}{}{}{} tabindex="-1" aria-selected="false" onfocusin="window.PullbellSelectElement(this)"><button class="row-open" type="button" tabindex="-1" data-row-shortcut="true" data-cmd="{}" onclick="send(this.dataset.cmd)"><div class="badge {}" title="{}" aria-label="{}">{}</div><div class="row-main"><div class="meta"><span class="repo">{}</span><span class="dot"></span><span>#{}</span><span class="dot"></span><span>{}</span></div><div class="title">{}</div></div></button><button class="row-copy row-action" type="button" title="Copy PR URL" aria-label="Copy PR URL" data-cmd="{}" onclick="event.stopPropagation(); send(this.dataset.cmd)">{}</button><button class="row-repo-mute row-action" type="button" title="Mute repository in Pullbell" aria-label="Mute repository in Pullbell" data-cmd="{}" onclick="event.stopPropagation(); send(this.dataset.cmd)">{}</button><div class="age">{}</div></div>"#,
        row_class,
        escape_attr(&command),
        escape_attr(&copy_command),
        escape_attr(&mute_repo_command),
        escape_attr(&item.repo),
        escape_attr(&reason),
        escape_attr(author),
        escape_attr(&item.title),
        escape_attr(&preview_meta),
        escape_attr(preview_body),
        done_attr,
        undo_attr,
        mute_attr,
        todo_attr,
        escape_attr(&command),
        badge_class,
        escape_attr(label),
        escape_attr(label),
        badge_icon,
        escape_html(repo),
        item.number,
        escape_html(label),
        escape_html(&item.title),
        escape_attr(&copy_command),
        copy_icon(),
        escape_attr(&mute_repo_command),
        muted_repository_icon(),
        escape_html(&age)
    )
}

fn item_kind_label(item: &PullRequestItem) -> &'static str {
    match item.kind {
        PrKind::ReviewRequested => "Review requested",
        PrKind::Notification => "Unread notification",
        PrKind::Authored => "Authored",
    }
}

fn item_icon_class(item: &PullRequestItem) -> &'static str {
    match item.kind {
        PrKind::ReviewRequested => "review",
        PrKind::Authored => "authored",
        PrKind::Notification => match item.reason.as_deref() {
            Some("ci_activity") => "ci",
            Some("mention" | "team_mention") => "mention",
            Some("security_alert") => "security",
            Some("state_change") => "state",
            Some("assign" | "assigned") => "assign",
            Some("subscribed" | "manual" | "invitation") => "muted",
            Some("review_requested") => "review",
            Some("comment") => "notify",
            _ => "notify",
        },
    }
}

fn item_icon(item: &PullRequestItem) -> &'static str {
    match item.kind {
        PrKind::ReviewRequested => pull_request_icon(),
        PrKind::Authored => authored_icon(),
        PrKind::Notification => match item.reason.as_deref() {
            Some("ci_activity") => ci_icon(),
            Some("mention" | "team_mention") => mention_icon(),
            Some("security_alert") => security_icon(),
            Some("state_change") => state_change_icon(),
            Some("assign" | "assigned") => assigned_icon(),
            Some("subscribed" | "manual" | "invitation") => bell_icon(),
            Some("review_requested") => pull_request_icon(),
            Some("comment") => comment_icon(),
            _ => comment_icon(),
        },
    }
}

fn preview_meta(item: &PullRequestItem, age: &str, reason: &str) -> String {
    let author = item
        .author
        .as_ref()
        .map(|author| format!(" by {author}"))
        .unwrap_or_default();

    format!(
        "{} #{} · {} · {}{}",
        item.repo, item.number, age, reason, author
    )
}

fn reason_label(item: &PullRequestItem) -> String {
    item.reason
        .as_deref()
        .map(humanize_reason)
        .unwrap_or_else(|| {
            match item.kind {
                PrKind::ReviewRequested => "Review requested",
                PrKind::Notification => "Notification",
                PrKind::Authored => "Author",
            }
            .to_string()
        })
}

fn humanize_reason(value: &str) -> String {
    value
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn short_filter_value(value: &str) -> &str {
    value.rsplit('/').next().unwrap_or(value)
}

fn notification_action_attr(name: &str, action: &str, item: &PullRequestItem) -> String {
    item.notification_thread_id
        .as_ref()
        .map(|thread_id| format!(" {name}=\"{action}:{}\"", escape_attr(thread_id)))
        .unwrap_or_default()
}

fn render_footer(snapshot: &AppState) -> String {
    let refresh_label = if snapshot.is_refreshing {
        "Refreshing"
    } else {
        "Refresh"
    };
    let mut html = format!(
        r#"<footer class="footer"><button class="tool primary" onclick="send('refresh')">{}</button><button class="tool" title="Open GitHub inbox" onclick="send('inbox')">Inbox</button>"#,
        refresh_label
    );

    html.push_str(r#"<div class="spacer"></div>"#);
    html.push_str(&format!(
        r#"<button class="tool icon" data-open-settings type="button" title="Settings" aria-label="Open settings" aria-controls="settings-view" aria-expanded="false" onclick="window.PullbellShowSettings()">{}</button>"#,
        settings_icon()
    ));
    html.push_str("</footer>");
    html
}

fn render_preview() -> String {
    r#"<aside id="preview" class="preview"><div class="preview-head"><div><div id="preview-title" class="preview-title"></div><div id="preview-meta" class="preview-meta"></div></div><button class="preview-close" onclick="window.PullbellHidePreview()" title="Close preview" aria-label="Close preview">x</button></div><div id="preview-body" class="preview-body"></div></aside>"#.to_string()
}

fn settings_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z"></path><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1A2 2 0 1 1 4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9 1.7 1.7 0 0 0-1.6-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.3 7A2 2 0 1 1 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3h.1a1.7 1.7 0 0 0 1-1.6V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.6h.1a1.7 1.7 0 0 0 1.9-.3l.1-.1A2 2 0 1 1 19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9v.1a1.7 1.7 0 0 0 1.6 1h.1a2 2 0 1 1 0 4H21a1.7 1.7 0 0 0-1.6.9Z"></path></svg>"#
}

fn back_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m15 18-6-6 6-6"></path></svg>"#
}

fn copy_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="13" height="13" rx="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>"#
}

fn pull_request_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M16 19.25a3.25 3.25 0 1 1 6.5 0 3.25 3.25 0 0 1-6.5 0Zm-14.5 0a3.25 3.25 0 1 1 6.5 0 3.25 3.25 0 0 1-6.5 0Zm0-14.5a3.25 3.25 0 1 1 6.5 0 3.25 3.25 0 0 1-6.5 0ZM4.75 3a1.75 1.75 0 1 0 .001 3.501A1.75 1.75 0 0 0 4.75 3Zm0 14.5a1.75 1.75 0 1 0 .001 3.501A1.75 1.75 0 0 0 4.75 17.5Zm14.5 0a1.75 1.75 0 1 0 .001 3.501 1.75 1.75 0 0 0-.001-3.501Z"></path><path d="M13.405 1.72a.75.75 0 0 1 0 1.06L12.185 4h4.065A3.75 3.75 0 0 1 20 7.75v8.75a.75.75 0 0 1-1.5 0V7.75a2.25 2.25 0 0 0-2.25-2.25h-4.064l1.22 1.22a.75.75 0 0 1-1.061 1.06l-2.5-2.5a.75.75 0 0 1 0-1.06l2.5-2.5a.75.75 0 0 1 1.06 0ZM4.75 7.25A.75.75 0 0 1 5.5 8v8A.75.75 0 0 1 4 16V8a.75.75 0 0 1 .75-.75Z"></path></svg>"#
}

fn authored_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="6" cy="6" r="3"></circle><circle cx="6" cy="18" r="3"></circle><path d="M6 9v6"></path><path d="M12 6h3a3 3 0 0 1 3 3v9"></path><path d="m15 15 3 3 3-3"></path></svg>"#
}

fn comment_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15a4 4 0 0 1-4 4H8l-5 3V7a4 4 0 0 1 4-4h10a4 4 0 0 1 4 4Z"></path></svg>"#
}

fn mention_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"></circle><path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8"></path></svg>"#
}

fn ci_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12a9 9 0 0 0-9-9 9.8 9.8 0 0 0-6.7 2.7L3 8"></path><path d="M3 3v5h5"></path><path d="M3 12a9 9 0 0 0 9 9 9.8 9.8 0 0 0 6.7-2.7L21 16"></path><path d="M16 16h5v5"></path></svg>"#
}

fn security_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 13c0 5-3.5 7.5-7.7 8.8a1 1 0 0 1-.6 0C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.2-2.7a1.2 1.2 0 0 1 1.6 0C14.5 3.8 17 5 19 5a1 1 0 0 1 1 1Z"></path><path d="M12 8v4"></path><path d="M12 16h.01"></path></svg>"#
}

fn state_change_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M8 18h1a4 4 0 0 0 4-4V6"></path><path d="M6 6h7"></path><path d="M3 18h5"></path><path d="m16 9-3-3 3-3"></path><path d="M17 18h4"></path></svg>"#
}

fn assigned_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"></path><circle cx="9" cy="7" r="4"></circle><path d="m16 11 2 2 4-4"></path></svg>"#
}

fn bell_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10.3 21a2 2 0 0 0 3.4 0"></path><path d="M18 8a6 6 0 1 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9"></path></svg>"#
}

fn muted_repository_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10.3 21a2 2 0 0 0 3.4 0"></path><path d="M17.8 17H21c0-2-3-2-3-9a6 6 0 0 0-9.3-5"></path><path d="M6.2 6.2C6.1 6.8 6 7.4 6 8c0 7-3 7-3 9h10"></path><path d="m3 3 18 18"></path></svg>"#
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

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_attr(value: &str) -> String {
    escape_html(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn item(kind: PrKind) -> PullRequestItem {
        PullRequestItem {
            id: "owner/repo#42".to_string(),
            repo: "owner/repo".to_string(),
            title: "Tighten notification layout".to_string(),
            url: "https://github.com/owner/repo/pull/42".to_string(),
            number: 42,
            updated_at: Some(Utc.timestamp_opt(7_200, 0).unwrap()),
            kind,
            notification_thread_id: None,
            author: Some("octo".to_string()),
            reason: Some("review_requested".to_string()),
            preview: Some("Review the updated notification layout before merging.".to_string()),
            locally_done: false,
        }
    }

    fn notification_item() -> PullRequestItem {
        PullRequestItem {
            notification_thread_id: Some("thread-42".to_string()),
            ..item(PrKind::Notification)
        }
    }

    #[test]
    fn renders_neat_like_sections_and_rows() {
        let snapshot = AppState {
            token_loaded: true,
            signed_in_as: Some("octo".to_string()),
            pull_requests: vec![
                item(PrKind::ReviewRequested),
                item(PrKind::Notification),
                item(PrKind::Authored),
            ],
            ..Default::default()
        };

        let body = render_body(&snapshot);

        assert!(body.contains(">Pinned<"));
        assert!(body.contains(">To do<"));
        assert!(body.contains(">Done<"));
        assert!(body.contains("Review requested"));
        assert!(body.contains("Unread notification"));
        assert!(body.contains("Authored"));
        assert!(body.contains("data-cmd=\"open:https://github.com/owner/repo/pull/42\""));
        assert!(body.contains("data-copy-cmd=\"copy-url:https://github.com/owner/repo/pull/42\""));
        assert!(body.contains("aria-label=\"Copy PR URL\""));
    }

    #[test]
    fn renders_keyboard_first_panel_behavior() {
        let snapshot = AppState {
            token_loaded: true,
            pull_requests: vec![item(PrKind::ReviewRequested)],
            ..Default::default()
        };

        let markup = html(&snapshot);

        assert!(markup.contains("data-selectable=\"true\""));
        assert!(markup.contains("window.PullbellSelect"));
        assert!(markup.contains("window.PullbellActivateSelected"));
        assert!(markup.contains("window.PullbellCopySelected"));
        assert!(markup.contains("window.PullbellActOnSelected"));
        assert!(markup.contains("window.PullbellTogglePreview"));
        assert!(markup.contains("rememberSelectionAfterDone"));
        assert!(markup.contains("restorePendingSelection"));
        assert!(markup.contains("pendingSelectionDoneCmd"));
        assert!(
            markup.contains("row.dataset.todoRow === &quot;true&quot;")
                || markup.contains(r#"row.dataset.todoRow === "true""#)
        );
        assert!(
            markup.contains("currentView === &quot;notifications&quot;")
                || markup.contains(r#"currentView === "notifications""#)
        );
        assert!(markup.contains("window.PullbellShowSettings"));
        assert!(markup.contains("window.PullbellShowNotifications"));
        assert!(markup.contains("window.PullbellToggleGroupByRepository"));
        assert!(markup.contains("window.PullbellSyncGroupByRepository"));
        assert!(markup.contains("ArrowDown"));
        assert!(
            markup.contains("currentView !== &quot;notifications&quot;")
                || markup.contains(r#"currentView !== "notifications""#)
        );
        assert!(
            markup.contains("currentView === &quot;settings&quot;")
                || markup.contains(r#"currentView === "settings""#)
        );
        assert!(
            markup.contains("event.target.dataset.rowShortcut !== &quot;true&quot;")
                || markup.contains("event.target.dataset.rowShortcut !== \"true\"")
        );
        assert!(markup.contains("^(SELECT|INPUT|TEXTAREA)$"));
        assert!(
            markup.contains("event.key === &quot; &quot;")
                || markup.contains(r#"event.key === " ""#)
        );
        assert!(markup.contains("key === &quot;c&quot;") || markup.contains(r#"key === "c""#));
        assert!(markup.contains("window.PullbellCopySelected()"));
        assert!(markup.contains("window.PullbellActOnSelected(\"done\")"));
        assert!(markup.contains("window.PullbellActOnSelected(\"undo\")"));
        assert!(markup.contains("window.PullbellActOnSelected(\"mute\")"));
        assert!(markup.contains("window.send(\"refresh\")"));
        assert!(markup.contains("window.send(\"inbox\")"));
        assert!(markup.contains("window.send(\"hide\")"));

        let form_guard = markup
            .find("SELECT|INPUT|TEXTAREA")
            .expect("form-control keyboard guard");
        let escape_handler = markup.find("key === \"escape\"").expect("escape handler");
        assert!(form_guard < escape_handler);
    }

    #[test]
    fn renders_settings_entrypoint_and_controls_view() {
        let snapshot = AppState {
            token_loaded: true,
            pull_requests: vec![item(PrKind::ReviewRequested)],
            ..Default::default()
        };

        let body = render_body(&snapshot);

        assert!(body.contains("aria-label=\"Open settings\""));
        assert!(body.contains("aria-controls=\"settings-view\""));
        assert!(body.contains("data-open-settings"));
        assert!(body.contains("id=\"settings-view\""));
        assert!(body.contains("data-panel-view=\"settings\""));
        assert!(body.contains("aria-label=\"Back to notifications\""));
        assert!(body.contains(">Settings<"));
        assert!(body.contains(">Account<"));
        assert!(body.contains(">Updates<"));
        assert!(body.contains(">Repositories<"));
        assert!(body.contains(">Muted repositories<"));
        assert!(body.contains(">Group by Repository<"));
        assert!(body.contains("data-group-by-repository"));
        assert!(body.contains("onclick=\"window.PullbellToggleGroupByRepository()\""));
        assert!(body.contains(">Muted<"));
        assert!(body.contains(">App<"));
        assert!(body.contains("Quit Pullbell"));
        assert!(body.contains("Check for updates"));
        assert!(body.contains("Open releases"));
        assert!(body.contains("Sign out"));
        assert!(body.matches("data-filter=\"repo\"").count() >= 2);
        assert!(body.matches("data-filter=\"reason\"").count() >= 2);
        assert!(body.matches("data-filter=\"author\"").count() >= 2);
    }

    #[test]
    fn muted_chips_preserve_distinct_repositories_with_same_short_name() {
        let snapshot = AppState {
            pull_requests: vec![
                PullRequestItem {
                    repo: "org-a/api".to_string(),
                    author: None,
                    reason: None,
                    ..item(PrKind::ReviewRequested)
                },
                PullRequestItem {
                    repo: "org-b/api".to_string(),
                    author: None,
                    reason: None,
                    ..item(PrKind::Notification)
                },
            ],
            ..Default::default()
        };

        let chips = render_muted_chips(&snapshot);

        assert_eq!(chips.matches(">api</span>").count(), 2);
    }

    #[test]
    fn renders_preview_and_filter_metadata() {
        let snapshot = AppState {
            token_loaded: true,
            pull_requests: vec![item(PrKind::ReviewRequested)],
            ..Default::default()
        };

        let body = render_body(&snapshot);

        assert!(body.contains("data-filter=\"repo\""));
        assert!(body.contains("data-filter=\"reason\""));
        assert!(body.contains("data-filter=\"author\""));
        assert!(body.contains("data-preview-title=\"Tighten notification layout\""));
        assert!(body.contains(
            "data-preview-body=\"Review the updated notification layout before merging.\""
        ));
        assert!(body.contains("id=\"preview\""));
        assert!(body.contains("aria-label=\"Close preview\""));
    }

    #[test]
    fn renders_copy_control_for_selected_rows() {
        let markup = html(&AppState {
            token_loaded: true,
            pull_requests: vec![item(PrKind::ReviewRequested)],
            ..Default::default()
        });

        assert!(markup.contains(".row-action"));
        assert!(markup.contains(".row.selected .row-action"));
        assert!(markup.contains(".row:focus-within .row-action"));
        assert!(markup.contains("pointer-events: none"));
        assert!(markup.contains("pointer-events: auto"));
        assert!(markup.contains("visibility: hidden"));
        assert!(markup.contains("visibility: visible"));
    }

    #[test]
    fn copy_control_keeps_native_button_keyboard_behavior() {
        let row = render_item(
            &item(PrKind::ReviewRequested),
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );
        let copy_button = row
            .split("<button class=\"row-copy row-action\"")
            .nth(1)
            .expect("copy button markup");

        assert!(row.contains(
            "class=\"row-open\" type=\"button\" tabindex=\"-1\" data-row-shortcut=\"true\""
        ));
        assert!(!copy_button.contains("data-row-shortcut=\"true\""));
        assert!(!copy_button.contains("tabindex=\"-1\""));
    }

    #[test]
    fn copy_control_renders_before_age() {
        let row = render_item(
            &item(PrKind::ReviewRequested),
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );
        let copy_index = row
            .find("class=\"row-copy row-action\"")
            .expect("copy button");
        let age_index = row.find("class=\"age\"").expect("age");

        assert!(copy_index < age_index);
    }

    #[test]
    fn renders_repository_mute_action_for_rows() {
        let row = render_item(
            &item(PrKind::ReviewRequested),
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );

        assert!(row.contains("data-mute-repo-cmd=\"mute-repo:owner/repo\""));
        assert!(row.contains("class=\"row-repo-mute row-action\""));
        assert!(row.contains("aria-label=\"Mute repository in Pullbell\""));
    }

    #[test]
    fn renders_repository_settings() {
        let snapshot = AppState {
            settings: pullbell::model::AppSettings {
                known_repositories: ["owner/repo".to_string(), "owner/other".to_string()].into(),
                muted_repositories: ["owner/repo".to_string()].into(),
            },
            ..Default::default()
        };

        let body = render_repository_settings(&snapshot);

        assert!(body.contains("Search repositories"));
        assert!(body.contains("data-repository-setting=\"owner/repo\""));
        assert!(body.contains("data-cmd=\"unmute-repo:owner/repo\""));
        assert!(body.contains("data-cmd=\"mute-repo:owner/other\""));
        assert!(body.contains("GitHub Watch settings are not changed."));
    }

    #[test]
    fn renders_icon_badges_instead_of_letter_badges() {
        let row = render_item(
            &item(PrKind::ReviewRequested),
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );

        assert!(row.contains("class=\"badge review\""));
        assert!(row.contains("aria-label=\"Review requested\""));
        assert!(row.contains("<svg viewBox=\"0 0 24 24\""));
        assert!(!row.contains(">R</div>"));
    }

    #[test]
    fn renders_notification_reason_specific_icons() {
        let row = render_item(
            &PullRequestItem {
                reason: Some("ci_activity".to_string()),
                ..notification_item()
            },
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );

        assert!(row.contains("class=\"badge ci\""));
        assert!(row.contains("aria-label=\"Unread notification\""));
    }

    #[test]
    fn notification_icon_does_not_use_merged_authored_reason() {
        let row = render_item(
            &PullRequestItem {
                reason: Some("author".to_string()),
                ..notification_item()
            },
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );

        assert!(row.contains("class=\"badge notify\""));
        assert!(!row.contains("class=\"badge authored\""));
    }

    #[test]
    fn renders_notification_thread_actions_when_available() {
        let row = render_item(&notification_item(), Utc.timestamp_opt(7_200, 0).unwrap());

        assert!(row.contains("data-done-cmd=\"done-pr:owner/repo#42\""));
        assert!(!row.contains("data-undo-cmd"));
        assert!(row.contains("data-mute-cmd=\"mute:thread-42\""));
        assert!(!row.contains("thread-42\"\""));
    }

    #[test]
    fn renders_done_action_without_notification_thread() {
        let row = render_item(
            &item(PrKind::ReviewRequested),
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );

        assert!(row.contains("data-done-cmd=\"done-pr:owner/repo#42\""));
        assert!(!row.contains("data-mute-cmd"));
    }

    #[test]
    fn renders_todo_rows_for_done_selection_restore() {
        let review_row = render_item(
            &item(PrKind::ReviewRequested),
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );
        let notification_row =
            render_item(&notification_item(), Utc.timestamp_opt(7_200, 0).unwrap());
        let authored_row = render_item(
            &item(PrKind::Authored),
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );
        let locally_done_row = render_item(
            &PullRequestItem {
                locally_done: true,
                ..item(PrKind::ReviewRequested)
            },
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );

        assert!(review_row.contains("data-todo-row=\"true\""));
        assert!(notification_row.contains("data-todo-row=\"true\""));
        assert!(!authored_row.contains("data-todo-row=\"true\""));
        assert!(!locally_done_row.contains("data-todo-row=\"true\""));
    }

    #[test]
    fn renders_local_done_review_requests_in_done_section() {
        let body = render_body(&AppState {
            token_loaded: true,
            pull_requests: vec![PullRequestItem {
                locally_done: true,
                ..item(PrKind::ReviewRequested)
            }],
            ..Default::default()
        });

        assert!(body.contains(">To do</span><span>0</span>"));
        assert!(body.contains(">Done</span><span>1</span>"));
        assert!(body.contains("Review requested"));
    }

    #[test]
    fn renders_undo_action_for_local_done_items() {
        let row = render_item(
            &PullRequestItem {
                locally_done: true,
                ..item(PrKind::ReviewRequested)
            },
            Utc.timestamp_opt(7_200, 0).unwrap(),
        );

        assert!(row.contains("data-undo-cmd=\"undo-pr:owner/repo#42\""));
    }

    #[test]
    fn escapes_dynamic_content() {
        let snapshot = AppState {
            last_error: Some("<script>alert('x')</script>".to_string()),
            ..Default::default()
        };

        let body = render_body(&snapshot);

        assert!(body.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"));
        assert!(!body.contains("<script>alert"));
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
    }
}
