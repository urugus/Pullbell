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

    pub(super) fn toggle_near(&mut self, rect: Rect) {
        if self.visible {
            self.hide();
        } else {
            self.show_near(rect);
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

    fn show_near(&mut self, rect: Rect) {
        let x = (rect.position.x + f64::from(rect.size.width) - PANEL_WIDTH + 16.0).max(8.0);
        let y = rect.position.y + f64::from(rect.size.height) + 8.0;

        self.window
            .set_outer_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        self.window.set_visible(true);
        self.window.set_focus();
        self.visible = true;
    }
}

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
  grid-template-columns: 34px minmax(0, 1fr) auto;
  gap: 10px;
  align-items: center;
  padding: 9px 10px;
  border: 0;
  border-radius: 10px;
  color: inherit;
  background: transparent;
  text-align: left;
  cursor: default;
}}
.row:hover {{
  background: #2c3036;
}}
.row:focus {{
  outline: none;
}}
.row:active {{
  background: #343940;
}}
.row.selected {{
  background: #313640;
  box-shadow: inset 0 0 0 1px rgba(255,255,255,.10);
}}
.badge {{
  width: 28px;
  height: 28px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  font-size: 12px;
  font-weight: 800;
  color: #f7f8fb;
}}
.badge.review {{ background: #8268ff; }}
.badge.notify {{ background: #26a0dc; }}
.badge.authored {{ background: #4f5966; }}
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

  function selectableRows() {{
    return Array.from(document.querySelectorAll("[data-selectable='true']"));
  }}

  function clamp(index, count) {{
    if (count === 0) return 0;
    return Math.max(0, Math.min(index, count - 1));
  }}

  window.PullbellSelect = function(index, shouldFocus) {{
    const rows = selectableRows().filter(function(row) {{ return !row.hidden; }});
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
    const index = selectableRows().filter(function(row) {{ return !row.hidden; }}).indexOf(element);
    if (index >= 0) window.PullbellSelect(index, false);
  }};

  window.PullbellActivateSelected = function() {{
    const row = selectableRows().filter(function(row) {{ return !row.hidden; }})[selectedIndex];
    if (row && row.dataset.cmd) window.send(row.dataset.cmd);
  }};

  window.PullbellActOnSelected = function(action) {{
    const row = selectableRows().filter(function(row) {{ return !row.hidden; }})[selectedIndex];
    if (!row) return;

    const command = action === "done" ? row.dataset.doneCmd : row.dataset.muteCmd;
    window.send(command || "missing-thread:" + action);
  }};

  window.PullbellShowPreview = function() {{
    const row = selectableRows().filter(function(row) {{ return !row.hidden; }})[selectedIndex];
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
    if (event.target && event.target.tagName === "BUTTON" && event.target.dataset.selectable !== "true") return;
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
    }} else if (event.key === " ") {{
      event.preventDefault();
      window.PullbellTogglePreview();
    }} else if (key === "d") {{
      event.preventDefault();
      window.PullbellActOnSelected("done");
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
        snapshot
            .pull_requests
            .iter()
            .filter(|item| item.kind.is_todo()),
        "All caught up",
    ));
    html.push_str(&render_section(
        "Done",
        done_count,
        snapshot
            .pull_requests
            .iter()
            .filter(|item| item.kind == PrKind::Authored),
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
    format!(
        r#"<main id="settings-view" class="content view settings" data-panel-view="settings"><div class="settings-head"><button class="tool icon" title="Back to notifications" aria-label="Back to notifications" onclick="window.PullbellShowNotifications()">{}</button><div><div class="settings-title">Controls</div><div class="settings-subtitle">Tune which pull request notifications stay in focus.</div></div></div><div class="settings-body"><section class="settings-section"><div class="settings-label">View</div><div class="setting-row"><div><div class="setting-name">Group by Repository</div><div class="setting-note">Preview control only. The notification list keeps the current order.</div></div><button class="switch" type="button" data-group-by-repository aria-label="Group by Repository" aria-pressed="false" onclick="window.PullbellToggleGroupByRepository()"></button></div></section><section class="settings-section"><div class="settings-label">Focus filters</div><div class="setting-note">These controls mirror the notification list filters.</div><div class="settings-filters">{}</div></section><section class="settings-section"><div class="settings-label">Muted</div><div class="setting-note">Available repositories, reasons, and users you can filter out for this session.</div>{}</section></div></main>"#,
        back_icon(),
        render_filter_controls(snapshot),
        render_muted_chips(snapshot)
    )
}

fn render_muted_chips(snapshot: &AppState) -> String {
    let values = sorted_values(
        snapshot
            .pull_requests
            .iter()
            .flat_map(|item| {
                [
                    Some(short_repo_name(&item.repo).to_string()),
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
    let (badge_class, badge_text, label) = match item.kind {
        PrKind::ReviewRequested => ("review", "R", "Review requested"),
        PrKind::Notification => ("notify", "N", "Unread notification"),
        PrKind::Authored => ("authored", "A", "Authored"),
    };
    let repo = short_repo_name(&item.repo);
    let age = item
        .updated_at
        .map(|updated_at| relative_age(updated_at, now))
        .unwrap_or_else(|| "unknown".to_string());
    let command = format!("open:{}", item.url);
    let done_attr = notification_action_attr("data-done-cmd", "done", item);
    let mute_attr = notification_action_attr("data-mute-cmd", "mute", item);
    let reason = reason_label(item);
    let author = item.author.as_deref().unwrap_or("");
    let preview_meta = preview_meta(item, &age, &reason);
    let preview_body = item
        .preview
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("No preview text available.");

    format!(
        r#"<button class="row" data-selectable="true" data-cmd="{}" data-repo="{}" data-reason="{}" data-author="{}" data-preview-title="{}" data-preview-meta="{}" data-preview-body="{}"{}{} tabindex="-1" aria-selected="false" onfocus="window.PullbellSelectElement(this)" onclick="send(this.dataset.cmd)"><div class="badge {}">{}</div><div class="main"><div class="meta"><span class="repo">{}</span><span class="dot"></span><span>#{}</span><span class="dot"></span><span>{}</span></div><div class="title">{}</div></div><div class="age">{}</div></button>"#,
        escape_attr(&command),
        escape_attr(&item.repo),
        escape_attr(&reason),
        escape_attr(author),
        escape_attr(&item.title),
        escape_attr(&preview_meta),
        escape_attr(preview_body),
        done_attr,
        mute_attr,
        badge_class,
        badge_text,
        escape_html(repo),
        item.number,
        escape_html(label),
        escape_html(&item.title),
        escape_html(&age)
    )
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

    if !snapshot.token_loaded && snapshot.pending_auth.is_none() {
        html.push_str(r#"<button class="tool" onclick="send('signin')">Sign in</button>"#);
    }

    if let Some(auth) = &snapshot.pending_auth {
        html.push_str(&format!(
            r#"<button class="tool" data-cmd="open:{}" onclick="send(this.dataset.cmd)">GitHub</button><button class="tool" onclick="send('copy-signin-code')">Copy</button>"#,
            escape_attr(&auth.verification_uri)
        ));
    }

    if let Some(update) = &snapshot.available_update {
        if snapshot.homebrew_cask_installed {
            html.push_str(
                r#"<button class="tool" onclick="send('update-homebrew')">Update</button>"#,
            );
        }
        html.push_str(&format!(
            r#"<button class="tool" data-cmd="open:{}" onclick="send(this.dataset.cmd)">Release</button>"#,
            escape_attr(&update.release_url)
        ));
    } else {
        html.push_str(r#"<button class="tool" onclick="send('check-updates')">Updates</button>"#);
        html.push_str(&format!(
            r#"<button class="tool" data-cmd="open:{}" onclick="send(this.dataset.cmd)">Release</button>"#,
            escape_attr(updater::RELEASES_URL)
        ));
    }

    html.push_str(r#"<div class="spacer"></div>"#);
    if snapshot.token_loaded {
        html.push_str(r#"<button class="tool" onclick="send('signout')">Sign out</button>"#);
    }
    html.push_str(&format!(
        r#"<button class="tool icon" data-open-settings type="button" title="Settings" aria-label="Open settings" aria-expanded="false" onclick="window.PullbellShowSettings()">{}</button>"#,
        settings_icon()
    ));
    html.push_str(r#"<button class="tool" onclick="send('quit')">Quit</button></footer>"#);
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
        assert!(markup.contains("window.PullbellActOnSelected"));
        assert!(markup.contains("window.PullbellTogglePreview"));
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
            markup.contains("event.target.dataset.selectable !== &quot;true&quot;")
                || markup.contains("event.target.dataset.selectable !== \"true\"")
        );
        assert!(markup.contains("^(SELECT|INPUT|TEXTAREA)$"));
        assert!(
            markup.contains("event.key === &quot; &quot;")
                || markup.contains(r#"event.key === " ""#)
        );
        assert!(markup.contains("window.PullbellActOnSelected(\"done\")"));
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
        assert!(body.contains("data-open-settings"));
        assert!(body.contains("id=\"settings-view\""));
        assert!(body.contains("data-panel-view=\"settings\""));
        assert!(body.contains("aria-label=\"Back to notifications\""));
        assert!(body.contains(">Controls<"));
        assert!(body.contains(">Group by Repository<"));
        assert!(body.contains("data-group-by-repository"));
        assert!(body.contains("onclick=\"window.PullbellToggleGroupByRepository()\""));
        assert!(body.contains(">Muted<"));
        assert!(body.matches("data-filter=\"repo\"").count() >= 2);
        assert!(body.matches("data-filter=\"reason\"").count() >= 2);
        assert!(body.matches("data-filter=\"author\"").count() >= 2);
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
    fn renders_notification_thread_actions_when_available() {
        let row = render_item(&notification_item(), Utc.timestamp_opt(7_200, 0).unwrap());

        assert!(row.contains("data-done-cmd=\"done:thread-42\""));
        assert!(row.contains("data-mute-cmd=\"mute:thread-42\""));
        assert!(!row.contains("thread-42\"\""));
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
