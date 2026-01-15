use base64::{engine::general_purpose::STANDARD, Engine as _};
use dioxus::prelude::*;
use rfd::AsyncFileDialog;
use std::{path::PathBuf, sync::Arc};

mod syntax;

#[derive(Clone, Copy, Debug, Default)]
struct Cursor {
    line: usize,
    col: usize,
}

#[derive(Clone, Debug)]
struct EditorState {
    lines: Arc<Vec<String>>,
    cursor: Cursor,
    scroll_x: f64,
    scroll_y: f64,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            lines: Arc::new(vec![String::new()]),
            cursor: Cursor::default(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
struct Tab {
    id: u64,
    path: Option<PathBuf>,
    language: String,
    dirty: bool,
    editor: EditorState,
}


impl Tab {
    fn new_untitled(id: u64) -> Self {
        Self {
            id,
            path: None,
            language: "plain".to_string(),
            dirty: false,
            editor: EditorState::default(),
        }
    }

    fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        let star = if self.dirty { "*" } else { "" };
        format!("{name}{star}")
    }
}

#[derive(Clone, Debug, PartialEq)]
enum PendingAction {
    None,
    CloseTab(usize),
    ExitApp,
}

/* ===== METRICS ===== */
const FONT_PX: f64 = 14.0;
const LINE_HEIGHT_EM: f64 = 1.4;
const PAD_X_PX: f64 = 10.0;
const PAD_Y_PX: f64 = 8.0;
const CHAR_WIDTH_RATIO: f64 = 0.60;

// Click forgiveness so you can click slightly left and still land on the intended column.
const CLICK_COL_BIAS_PX: f64 = 2.0;

fn line_px() -> f64 {
    (FONT_PX * LINE_HEIGHT_EM).round()
}

fn char_px() -> f64 {
    FONT_PX * CHAR_WIDTH_RATIO
}


fn visible_range(scroll_top: f64, viewport_h: f64, total_lines: usize) -> (usize, usize, f64, f64) {
    if total_lines == 0 {
        return (0, 0, 0.0, 0.0);
    }
    let lp = line_px();
    // Add a buffer so scrolling doesn't cause constant re-renders.
    let buffer: usize = 20;
    let start = ((scroll_top / lp).floor() as isize).max(0) as usize;
    let visible = ((viewport_h / lp).ceil() as usize).saturating_add(buffer);
    let end = (start + visible).min(total_lines);

    let top_h = (start as f64) * lp;
    let bottom_h = ((total_lines - end) as f64) * lp;
    (start, end, top_h, bottom_h)
}

fn join_lines(lines: &[String]) -> String {
    lines.join("\n")
}

fn split_lines_vec(text: &str) -> Vec<String> {
    let mut v: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    if v.is_empty() {
        v.push(String::new());
    }
    v
}

fn split_lines(text: &str) -> Arc<Vec<String>> {
    Arc::new(split_lines_vec(text))
}

fn next_tab_id(tabs: &[Tab]) -> u64 {
    tabs.iter().map(|t| t.id).max().unwrap_or(0).saturating_add(1)
}

fn find_open_tab_index(tabs: &[Tab], path: &PathBuf) -> Option<usize> {
    tabs.iter().position(|t| t.path.as_ref() == Some(path))
}

fn set_active_tab_editor<F: FnOnce(&mut Tab)>(mut tabs: Signal<Vec<Tab>>, active: Signal<usize>, f: F) {
    let mut v = tabs();
    let idx = active();
    if let Some(t) = v.get_mut(idx) {
        f(t);
        tabs.set(v);
    }
}


fn maybe_disable_highlighting(path: &PathBuf, language: String) -> String {
    // Disable syntax highlighting for huge files because rendering and tokenising
    // a million lines is a hobby for people who hate themselves.
    const DISABLE_AT_BYTES: u64 = 2_000_000; // 2 MB
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() >= DISABLE_AT_BYTES {
            return "plain".to_string();
        }
    }
    language
}

/* ===== DIRECTORY FUNCTIONS ===== */

async fn open_directory(
    mut current_dir: Signal<Option<PathBuf>>,
    mut dir_contents: Signal<Vec<(String, PathBuf)>>,
    mut status: Signal<String>,
) {
    if let Some(handle) = AsyncFileDialog::new().pick_folder().await {
        let path = handle.path().to_path_buf();
        match list_directory_contents(&path) {
            Ok(contents) => {
                current_dir.set(Some(path.clone()));
                dir_contents.set(contents);
                status.set(format!("Opened directory: {}", path.display()));
            }
            Err(err) => status.set(format!("Failed to list directory: {err}")),
        }
    }
}

fn list_directory_contents(path: &PathBuf) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut contents = Vec::new();

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        contents.push((name, p));
    }

    // Sort by name
    contents.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(contents)
}

fn close_directory(
    mut current_dir: Signal<Option<PathBuf>>,
    mut dir_contents: Signal<Vec<(String, PathBuf)>>,
    mut status: Signal<String>,
) {
    current_dir.set(None);
    dir_contents.set(Vec::new());
    status.set("Directory closed".to_string());
}

/// Build CSS + bundled font
/// Place JetBrainsMono-Regular.ttf at: assets/fonts/JetBrainsMono-Regular.ttf
fn bundled_css() -> String {
    const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
    let b64 = STANDARD.encode(FONT_BYTES);

    let template = r#"
@font-face {
  font-family: "BundledMono";
  src: url("data:font/ttf;base64,__B64__") format("truetype");
}

:root {
  --bg: #0f1117;
  --panel: #0b0d12;
  --text: #e6e6e6;
  --muted: #8a93a6;
  --border: #232a3a;
  --linehl: rgba(88, 135, 255, 0.12);
  --caret: rgba(230, 230, 230, 0.9);

  --pad-x: __PAD_X__px;
  --pad-y: __PAD_Y__px;
  --line-h: __LINE_PX__px;
  --font-size: __FONT_PX__px;

  --menubar-h: 34px;
  --tabbar-h: 30px;
}

* {
  font-family: "BundledMono", monospace;
  font-variant-ligatures: none;
  font-feature-settings: "liga" 0, "calt" 0;
  box-sizing: border-box;
}

html, body {
  margin: 0;
  height: 100%;
  background: var(--bg);
  color: var(--text);
  font-size: var(--font-size);
}

#main, .app {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
}

/* ===== MENU BAR ===== */
.menubar {
  height: var(--menubar-h);
  display: flex;
  align-items: center;
  padding: 0 10px;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
  user-select: none;
}

.menu {
  position: relative;
}

.menu-button {
  height: 26px;
  padding: 0 10px;
  background: transparent;
  border: 1px solid transparent;
  color: var(--text);
  cursor: pointer;
}

.menu-button:hover {
  border-color: var(--border);
  background: rgba(255,255,255,0.03);
}

.dropdown {
  position: absolute;
  top: 30px;
  left: 0;
  min-width: 250px;
  background: #0c0f16;
  border: 1px solid var(--border);
  box-shadow: 0 8px 30px rgba(0,0,0,0.35);
  padding: 6px;
  z-index: 2000;
}

.menu-item {
  width: 100%;
  text-align: left;
  padding: 8px 10px;
  background: transparent;
  border: none;
  color: var(--text);
  cursor: pointer;
}

.menu-item:hover {
  background: rgba(255,255,255,0.06);
}

.menu-sep {
  height: 1px;
  background: var(--border);
  margin: 6px 0;
}

.file-indicator {
  margin-left: 12px;
  color: var(--muted);
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ===== TABS ===== */
.tabbar {
  height: var(--tabbar-h);
  display: flex;
  align-items: stretch;
  background: #0c0f16;
  border-bottom: 1px solid var(--border);
  overflow-x: auto;
  overflow-y: hidden;
  user-select: none;
}

.tab {
  height: 100%;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  border-right: 1px solid var(--border);
  color: var(--muted);
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
}

.tab:hover {
  background: rgba(255,255,255,0.04);
  color: var(--text);
}

.tab.active {
  background: rgba(255,255,255,0.06);
  color: var(--text);
}

.tab-title {
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tab-close {
  width: 18px;
  height: 18px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid transparent;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  padding: 0;
}

.tab-close:hover {
  border-color: var(--border);
  background: rgba(255,255,255,0.05);
  color: var(--text);
}

.tab-plus {
  height: 100%;
  width: 34px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--muted);
  border-right: 1px solid var(--border);
  flex-shrink: 0;
}

.tab-plus:hover {
  background: rgba(255,255,255,0.04);
  color: var(--text);
}

/* ===== EDITOR LAYOUT ===== */
.editor-wrap {
  flex: 1;
  min-height: 0;
  display: flex;
  overflow: hidden;
}

.row {
  display: flex;
  flex: 1;
  min-height: 0;
}

.scroll {
  flex: 1;
  display: flex;
  overflow: auto;
  outline: none;
  min-width: 0;
}

.editor-content {
  display: flex;
  min-height: 100%;
}

.gutter {
  width: 56px;
  background: var(--panel);
  border-right: 1px solid var(--border);
  padding: var(--pad-y) 0;
  color: var(--muted);
  user-select: none;
  flex-shrink: 0;
}

.ln {
  text-align: right;
  padding-right: 8px;
  height: var(--line-h);
}

.ln.active {
  background: var(--linehl);
  color: #cfe0ff;
}

.textpane {
  position: relative;
  flex: 1;
  padding: var(--pad-y) var(--pad-x);
  white-space: pre;
  line-height: var(--line-h);
  min-width: 400px;
}

/* Make clicks hit .textpane, not child .line divs */
.line {
  height: var(--line-h);
  pointer-events: none;
  white-space: pre;
  tab-size: 4;
}

.line.active {
  background: var(--linehl);
}

.caret {
  position: absolute;
  width: 2px;
  height: var(--line-h);
  background: var(--caret);
  pointer-events: none;
}

/* ===== SIDEBAR ===== */
.sidebar-resize {
  width: 280px;
  min-width: 180px;
  max-width: 620px;
  background: var(--panel);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  user-select: none;
  flex-shrink: 0;

  /* We resize this via state + drag handle for consistent behavior in Dioxus Desktop */
  overflow: hidden;
}

.sidebar-handle {
  width: 6px;
  cursor: ew-resize;
  background: transparent;
  flex-shrink: 0;
}

.sidebar-handle:hover {
  background: rgba(255,255,255,0.06);
}

.sidebar {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
}

.sidebar-collapsed {
  width: 24px;
  background: var(--panel);
  border-right: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--muted);
  font-size: 12px;
  flex-shrink: 0;
}

.sidebar-collapsed:hover {
  background: rgba(255,255,255,0.05);
  color: var(--text);
}

.sidebar-header {
  height: 34px;
  padding: 0 12px;
  background: rgba(0,0,0,0.2);
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
  flex-shrink: 0;
}

.sidebar-header:hover {
  background: rgba(255,255,255,0.03);
}

.sidebar-title {
  font-size: 12px;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sidebar-collapse-btn {
  background: transparent;
  border: none;
  color: var(--muted);
  cursor: pointer;
  font-size: 16px;
  padding: 0 4px;
}

.sidebar-collapse-btn:hover {
  color: var(--text);
}

.sidebar-contents {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
}

.sidebar-item {
  width: 100%;
  text-align: left;
  padding: 6px 12px;
  background: transparent;
  border: none;
  color: var(--text);
  font-size: 12px;
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sidebar-item:hover {
  background: rgba(255,255,255,0.06);
}

.sidebar-empty {
  padding: 20px;
  text-align: center;
  color: var(--muted);
  font-size: 12px;
}

/* ===== CONFIRM MODAL ===== */
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 4000;
}

.modal {
  width: 520px;
  background: #0c0f16;
  border: 1px solid var(--border);
  padding: 14px;
  box-shadow: 0 12px 50px rgba(0,0,0,0.5);
}

.modal-title {
  margin-bottom: 8px;
  color: var(--text);
  font-size: 14px;
}

.modal-sub {
  margin-bottom: 12px;
  color: var(--muted);
  font-size: 12px;
}

.modal-actions {
  display: flex;
  gap: 10px;
  justify-content: flex-end;
}

.btn {
  padding: 8px 12px;
  border: 1px solid var(--border);
  cursor: pointer;
  background: transparent;
  color: var(--text);
}

.btn:hover {
  background: rgba(255,255,255,0.06);
}

.btn-danger {
  background: rgba(255,80,80,0.12);
  border-color: rgba(255,80,80,0.35);
}

.btn-primary {
  background: rgba(88,135,255,0.18);
  border-color: rgba(88,135,255,0.35);
}

/* ===== SCROLLBARS ===== */
.scroll {
  scrollbar-gutter: stable;
}

.scroll::-webkit-scrollbar {
  width: 12px;
  height: 12px;
}

.scroll::-webkit-scrollbar-track {
  background: #0b0d12;
}

.scroll::-webkit-scrollbar-thumb {
  background-color: #2a3246;
  border-radius: 8px;
  border: 3px solid #0b0d12;
}

.scroll::-webkit-scrollbar-thumb:hover{
  background-color: #3a4563;
}

.scroll::-webkit-scrollbar-corner {
  background: #0b0d12;
}

:root {
  color-scheme: dark;
}
"#;

    template
        .replace("__B64__", &b64)
        .replace("__PAD_X__", &format!("{PAD_X_PX}"))
        .replace("__PAD_Y__", &format!("{PAD_Y_PX}"))
        .replace("__LINE_PX__", &format!("{}", line_px()))
        .replace("__FONT_PX__", &format!("{FONT_PX}"))
}

/* ===== FILE OPS (TABS) ===== */

fn create_new_tab(mut tabs: Signal<Vec<Tab>>, mut active_tab: Signal<usize>, mut status: Signal<String>) {
    let mut v = tabs();
    let id = next_tab_id(&v);
    v.push(Tab::new_untitled(id));
    let new_idx = v.len().saturating_sub(1);
    tabs.set(v);
    active_tab.set(new_idx);
    status.set("New tab".to_string());
}

async fn open_dialog_add_tab(
    mut tabs: Signal<Vec<Tab>>,
    mut active_tab: Signal<usize>,
    mut status: Signal<String>,
) {
    if let Some(handle) = AsyncFileDialog::new().pick_file().await {
        let path = handle.path().to_path_buf();

        // already open? just focus
        if let Some(idx) = find_open_tab_index(&tabs(), &path) {
            active_tab.set(idx);
            status.set(format!("Focused {}", path.display()));
            return;
        }

        status.set(format!("Opening {} ...", path.display()));

        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                let lines = split_lines(&contents);

                let mut v = tabs();
                let id = next_tab_id(&v);
                let language = maybe_disable_highlighting(
                    &path,
                    crate::syntax::detect_language_from_path(&path),
                );

                v.push(Tab {
                    id,
                    path: Some(path.clone()),
                    language,
                    dirty: false,
                    editor: EditorState {
                        lines,
                        cursor: Cursor { line: 0, col: 0 },
                        scroll_x: 0.0,
                        scroll_y: 0.0,
                    },
                });

                let new_idx = v.len().saturating_sub(1);
                tabs.set(v);
                active_tab.set(new_idx);
                status.set(format!("Opened {}", path.display()));
            }
            Err(err) => status.set(format!("Open failed: {err}")),
        }
    }
}


async fn open_path_in_tab(
    mut tabs: Signal<Vec<Tab>>,
    mut active_tab: Signal<usize>,
    mut status: Signal<String>,
    path: PathBuf,
) {
    if path.is_dir() {
        status.set(format!("Directory click does nothing (yet): {}", path.display()));
        return;
    }

    if let Some(idx) = find_open_tab_index(&tabs(), &path) {
        active_tab.set(idx);
        status.set(format!("Focused {}", path.display()));
        return;
    }

    status.set(format!("Opening {} ...", path.display()));
    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            let mut v = tabs();
            let id = next_tab_id(&v);
            let language = maybe_disable_highlighting(&path, crate::syntax::detect_language_from_path(&path));
            v.push(Tab {
                id,
                path: Some(path.clone()),
                language,
                dirty: false,
                editor: EditorState {
                    lines: split_lines(&contents),
                    cursor: Cursor { line: 0, col: 0 },
                    scroll_x: 0.0,
                    scroll_y: 0.0,
                },
            });
            let new_idx = v.len().saturating_sub(1);
            tabs.set(v);
            active_tab.set(new_idx);
            status.set(format!("Opened {}", path.display()));
        }
        Err(err) => status.set(format!("Open failed: {err}")),
    }
}

async fn save_tab_to_path(
    mut tabs: Signal<Vec<Tab>>,
    tab_index: usize,
    mut status: Signal<String>,
    path: PathBuf,
) {
    let mut v = tabs();
    if tab_index >= v.len() {
        return;
    }

    let text = v[tab_index].editor.lines.as_ref().join("\n");
    match std::fs::write(&path, text) {
        Ok(()) => {
            v[tab_index].path = Some(path.clone());
            v[tab_index].language = crate::syntax::detect_language_from_path(&path);
            v[tab_index].dirty = false;
            tabs.set(v);
            status.set(format!("Saved {}", path.display()));
        }
        Err(err) => status.set(format!("Save failed: {err}")),
    }
}

async fn save_active_or_save_as(
    tabs: Signal<Vec<Tab>>,
    active_tab: Signal<usize>,
    status: Signal<String>,
) {
    let idx = active_tab();
    let v = tabs();
    if idx >= v.len() {
        return;
    }

    if let Some(p) = v[idx].path.clone() {
        save_tab_to_path(tabs, idx, status, p).await;
        return;
    }

    if let Some(handle) = AsyncFileDialog::new().save_file().await {
        let path = handle.path().to_path_buf();
        save_tab_to_path(tabs, idx, status, path).await;
    }
}

async fn save_as_active(tabs: Signal<Vec<Tab>>, active_tab: Signal<usize>, status: Signal<String>) {
    let idx = active_tab();
    let v = tabs();
    if idx >= v.len() {
        return;
    }

    if let Some(handle) = AsyncFileDialog::new().save_file().await {
        let path = handle.path().to_path_buf();
        save_tab_to_path(tabs, idx, status, path).await;
    }
}

fn close_tab_immediately(mut tabs: Signal<Vec<Tab>>, mut active_tab: Signal<usize>, idx: usize) {
    let mut v = tabs();
    if v.is_empty() {
        v.push(Tab::new_untitled(1));
        tabs.set(v);
        active_tab.set(0);
        return;
    }

    if idx >= v.len() {
        return;
    }

    v.remove(idx);

    if v.is_empty() {
        v.push(Tab::new_untitled(1));
        tabs.set(v);
        active_tab.set(0);
        return;
    }

    // keep active in range
    let mut a = active_tab();
    if a >= v.len() {
        a = v.len() - 1;
    }

    tabs.set(v);
    active_tab.set(a);
}

pub fn app() -> Element {
    let css = bundled_css();

    // Tabs
    let mut tabs = use_signal(|| vec![Tab::new_untitled(1)]);
    let mut active_tab = use_signal(|| 0usize);

    // UI
    let mut file_open = use_signal(|| false);
    let mut status = use_signal(|| "".to_string());

    // Sidebar (directory)
    let mut current_dir = use_signal(|| Option::<PathBuf>::None);
    let mut dir_contents = use_signal(|| Vec::<(String, PathBuf)>::new());
    let mut sidebar_collapsed = use_signal(|| false);
    let mut sidebar_width = use_signal(|| 280.0f64);
    let mut sidebar_resizing = use_signal(|| false);
    let mut sidebar_resize_start_x = use_signal(|| 0.0f64);
    let mut sidebar_resize_start_w = use_signal(|| 280.0f64);

    // Confirm modal
    let mut confirm_open = use_signal(|| false);
    let mut pending_action = use_signal(|| PendingAction::None);

    // for smooth scrolling (currently not used heavily, but kept)
    let mut scroll_top = use_signal(|| 0.0f64);
    let mut viewport_h = use_signal(|| 600.0f64);

    // derived
    let active_idx = active_tab();
    let active_title = tabs()
        .get(active_idx)
        .map(|t| t.title())
        .unwrap_or_else(|| "Untitled".to_string());

    let active_language = tabs()
        .get(active_idx)
        .map(|t| t.language.clone())
        .unwrap_or_else(|| "plain".to_string());

    let active_dirty = tabs()
        .get(active_idx)
        .map(|t| t.dirty)
        .unwrap_or(false);

    let active_path = tabs()
        .get(active_idx)
        .and_then(|t| t.path.clone());

    rsx! {
        style { "{css}" }

        div {
            class: "app",

            // click anywhere closes file dropdown
            onclick: move |_| {
                if file_open() {
                    file_open.set(false);
                }
            },

            // ===== Menu bar =====
            div { class: "menubar",
                div { class: "menu",
                    button {
                        class: "menu-button",
                        onclick: move |e| {
                            e.stop_propagation();
                            file_open.set(!file_open());
                        },
                        "File"
                    }

                    if file_open() {
                        div {
                            class: "dropdown",
                            onclick: move |e| e.stop_propagation(),

                            // New tab
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);
                                    create_new_tab(tabs.clone(), active_tab.clone(), status.clone());
                                },
                                "New Tab - Ctrl+N"
                            }

                            // Open file...
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);
                                    let tabs2 = tabs.clone();
                                    let act2 = active_tab.clone();
                                    let mut status2 = status.clone();
                                    spawn(async move { open_dialog_add_tab(tabs2, act2, status2).await; });
                                },
                                "Open - Ctrl+O"
                            }

                            // Open directory
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);
                                    let current_dir2 = current_dir.clone();
                                    let dir_contents2 = dir_contents.clone();
                                    let mut status2 = status.clone();
                                    spawn(async move { open_directory(current_dir2, dir_contents2, status2).await; });
                                },
                                "Open Directory - Ctrl+Shift+O"
                            }

                            // Save
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);
                                    let tabs2 = tabs.clone();
                                    let act2 = active_tab.clone();
                                    let mut status2 = status.clone();
                                    spawn(async move { save_active_or_save_as(tabs2, act2, status2).await; });
                                },
                                "Save - Ctrl+S"
                            }

                            // Save As
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);
                                    let tabs2 = tabs.clone();
                                    let act2 = active_tab.clone();
                                    let mut status2 = status.clone();
                                    spawn(async move { save_as_active(tabs2, act2, status2).await; });
                                },
                                "Save As - Ctrl+Shift+S"
                            }

                            div { class: "menu-sep" }

                            // Close directory
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);
                                    close_directory(current_dir.clone(), dir_contents.clone(), status.clone());
                                },
                                "Close Directory - Ctrl+Shift+C"
                            }

                            div { class: "menu-sep" }

                            // Exit
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);

                                    // If the active tab is dirty, confirm. (Yes, this is basic. No, it won't babysit every dirty tab.)
                                    if active_dirty {
                                        pending_action.set(PendingAction::ExitApp);
                                        confirm_open.set(true);
                                        return;
                                    }

                                    dioxus_desktop::window().close();
                                },
                                "Exit - Ctrl+Q"
                            }
                        }
                    }
                }

                div { class: "file-indicator", "{active_title}" }
                div { class: "file-indicator", "{status()}" }
            }

            // ===== Tabs =====
            div { class: "tabbar",
                // plus
                div {
                    class: "tab-plus",
                    onclick: move |_| create_new_tab(tabs.clone(), active_tab.clone(), status.clone()),
                    "+"
                }

                for (idx, tab) in tabs().iter().enumerate() {
                    div {
                        class: if idx == active_tab() { "tab active" } else { "tab" },
                        onclick: {
                            let idx = idx;
                            move |_| active_tab.set(idx)
                        },

                        span { class: "tab-title", "{tab.title()}" }

                        button {
                            class: "tab-close",
                            onclick: {
                                let idx = idx;
                                let tabs2 = tabs.clone();
                                let act2 = active_tab.clone();
                                let mut confirm2 = confirm_open.clone();
                                let mut pending2 = pending_action.clone();
                                move |e| {
                                    e.stop_propagation();
                                    let v = tabs2();
                                    if idx >= v.len() {
                                        return;
                                    }
                                    if v[idx].dirty {
                                        pending2.set(PendingAction::CloseTab(idx));
                                        confirm2.set(true);
                                    } else {
                                        close_tab_immediately(tabs2.clone(), act2.clone(), idx);
                                    }
                                }
                            },
                            "×"
                        }
                    }
                }
            }

            // ===== Editor =====
            div {
                class: "editor-wrap",

                // Sidebar resize drag handling
                onmousemove: move |e| {
                    if !sidebar_resizing() {
                        return;
                    }

                    let x = e.data().coordinates().client().x;
                    let dx = x - sidebar_resize_start_x();
                    let new_w = (sidebar_resize_start_w() + dx).clamp(180.0, 620.0);
                    sidebar_width.set(new_w);
                    e.prevent_default();
                },
                onmouseup: move |_| sidebar_resizing.set(false),
                onmouseleave: move |_| sidebar_resizing.set(false),
                div {
                    class: "row",

                    // Sidebar (left)
                    if !sidebar_collapsed() {
                        div {
                            class: "sidebar-resize",
                            style: "width: {sidebar_width()}px;",
                            div { class: "sidebar",
                                div {
                                    class: "sidebar-header",
                                    onclick: move |_| sidebar_collapsed.set(true),

                                    div {
                                        class: "sidebar-title",
                                        {
                                            if let Some(p) = current_dir() {
                                                rsx!("{p.display()}")
                                            } else {
                                                rsx!("No directory")
                                            }
                                        }
                                    }

                                    button { class: "sidebar-collapse-btn", "×" }
                                }

                                div { class: "sidebar-contents",
                                    if current_dir().is_some() {
                                        for (name, path) in dir_contents().iter() {
                                            button {
                                                class: "sidebar-item",
                                                onclick: {
                                                    let tabs2 = tabs.clone();
                                                    let act2 = active_tab.clone();
                                                    let mut status2 = status.clone();
                                                    let p = path.clone();
                                                    let n = name.clone();
                                                    move |_| {
                                                        if p.is_dir() {
                                                            status2.set(format!("Directory: {n}"));
                                                        } else {
                                                            let tabs3 = tabs2.clone();
                                                            let act3 = act2.clone();
                                                            let status3 = status2.clone();
                                                            let p2 = p.clone();
                                                            spawn(async move { open_path_in_tab(tabs3, act3, status3, p2).await; });
                                                        }
                                                    }
                                                },
                                                if path.is_dir() { "[DIR] " } else { "[FILE] " }
                                                "{name}"
                                            }
                                        }
                                    } else {
                                        div { class: "sidebar-empty", "No directory open" }
                                    }
                                }
                            }
                        }

                        // Drag handle to resize the sidebar
                        div {
                            class: "sidebar-handle",
                            onmousedown: move |e| {
                                e.stop_propagation();
                                sidebar_resizing.set(true);
                                sidebar_resize_start_x.set(e.data().coordinates().client().x);
                                sidebar_resize_start_w.set(sidebar_width());
                                e.prevent_default();
                            },
                        }
                    } else if sidebar_collapsed() && current_dir().is_some() {
                        div {
                            class: "sidebar-collapsed",
                            onclick: move |_| sidebar_collapsed.set(false),
                            "▶"
                        }
                    }

                    // Editor content (gutter + textpane)
                    div {
                        class: "scroll",
                        tabindex: "0",
                        id: "scrollpane",

                        onscroll: move |e| {
                            let d = e.data();
                            scroll_top.set(d.scroll_top() as f64);
                            viewport_h.set(d.client_height() as f64);
                        },

                        onkeydown: move |e| {
                            let kd = e.data();
                            let m = kd.modifiers();
                            let ctrl = m.ctrl() || m.meta();
                            let shift = m.shift();
                            let key = kd.key();

                            if ctrl {
                                if let Key::Character(c) = key {
                                    match (shift, c.to_lowercase().as_str()) {
                                        // Ctrl/Cmd + N : New tab
                                        (false, "n") => {
                                            create_new_tab(tabs.clone(), active_tab.clone(), status.clone());
                                            e.prevent_default();
                                            e.stop_propagation();
                                            return;
                                        }
                                        // Ctrl/Cmd + O : Open file
                                        (false, "o") => {
                                            let tabs2 = tabs.clone();
                                            let act2 = active_tab.clone();
                                            let mut status2 = status.clone();
                                            spawn(async move { open_dialog_add_tab(tabs2, act2, status2).await; });
                                            e.prevent_default();
                                            e.stop_propagation();
                                            return;
                                        }
                                        // Ctrl/Cmd + Shift + O : Open directory
                                        (true, "o") => {
                                            let current_dir2 = current_dir.clone();
                                            let dir_contents2 = dir_contents.clone();
                                            let mut status2 = status.clone();
                                            spawn(async move { open_directory(current_dir2, dir_contents2, status2).await; });
                                            e.prevent_default();
                                            e.stop_propagation();
                                            return;
                                        }
                                        // Ctrl/Cmd + Shift + C : Close directory
                                        (true, "c") => {
                                            close_directory(current_dir.clone(), dir_contents.clone(), status.clone());
                                            e.prevent_default();
                                            e.stop_propagation();
                                            return;
                                        }
                                        // Ctrl/Cmd + S : Save
                                        (false, "s") => {
                                            let tabs2 = tabs.clone();
                                            let act2 = active_tab.clone();
                                            let mut status2 = status.clone();
                                            spawn(async move { save_active_or_save_as(tabs2, act2, status2).await; });
                                            e.prevent_default();
                                            e.stop_propagation();
                                            return;
                                        }
                                        // Ctrl/Cmd + Shift + S : Save As
                                        (true, "s") => {
                                            let tabs2 = tabs.clone();
                                            let act2 = active_tab.clone();
                                            let mut status2 = status.clone();
                                            spawn(async move { save_as_active(tabs2, act2, status2).await; });
                                            e.prevent_default();
                                            e.stop_propagation();
                                            return;
                                        }
                                        // Ctrl/Cmd + B : Toggle sidebar
                                        (false, "b") => {
                                            sidebar_collapsed.set(!sidebar_collapsed());
                                            e.prevent_default();
                                            e.stop_propagation();
                                            return;
                                        }
                                        // Ctrl/Cmd + W : Close tab
                                        (false, "w") => {
                                            let idx = active_tab();
                                            let v = tabs();
                                            if idx < v.len() {
                                                if v[idx].dirty {
                                                    pending_action.set(PendingAction::CloseTab(idx));
                                                    confirm_open.set(true);
                                                } else {
                                                    close_tab_immediately(tabs.clone(), active_tab.clone(), idx);
                                                }
                                            }
                                            e.prevent_default();
                                            e.stop_propagation();
                                            return;
                                        }
                                        // Ctrl/Cmd + Q : Quit
                                        (false, "q") => {
                                            if active_dirty {
                                                pending_action.set(PendingAction::ExitApp);
                                                confirm_open.set(true);
                                            } else {
                                                dioxus_desktop::window().close();
                                            }
                                            e.prevent_default();
                                            e.stop_propagation();
                                            return;
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            // ===== Editor typing =====
                            let key = e.data().key();
                            let idx = active_tab();

                            set_active_tab_editor(tabs.clone(), active_tab.clone(), |t| {
                                let changed = handle_key(&mut t.editor, key);
                                if changed {
                                    t.dirty = true;
                                }
                            });

                            // status line hint
                            if idx < tabs().len() {
                                // nothing
                            }

                            e.prevent_default();
                            e.stop_propagation();
                        },

                        div { class: "editor-content",
                            // gutter
                            {
                                let v = tabs();
                                let idx = active_tab();

                                let (lines, cursor_line) = if idx < v.len() {
                                    (v[idx].editor.lines.clone(), v[idx].editor.cursor.line)
                                } else {
                                    (Arc::new(vec![String::new()]), 0usize)
                                };

                                let total = lines.len();
                                let (start, end, top_h, bottom_h) =
                                    visible_range(scroll_top(), viewport_h(), total);

                                rsx!(
                                    div { class: "gutter",
                                        div { style: "height: {top_h}px;" }
                                        for i in start..end {
                                            div {
                                                class: if i == cursor_line { "ln active" } else { "ln" },
                                                "{i + 1}"
                                            }
                                        }
                                        div { style: "height: {bottom_h}px;" }
                                    }
                                )
                            }

// text pane
                            div {
                                class: "textpane",

                                onclick: move |e| {
                                    let p = e.data().coordinates().element();
                                    let content_x = (p.x - PAD_X_PX) + CLICK_COL_BIAS_PX;
                                    let content_y = (p.y - PAD_Y_PX) + scroll_top();

                                    set_active_tab_editor(tabs.clone(), active_tab.clone(), |t| {
                                        let s = &mut t.editor;
                                        if s.lines.is_empty() {
                                            lines_mut(s).push(String::new());
                                        }

                                        let mut line = if content_y <= 0.0 {
                                            0
                                        } else {
                                            (content_y / line_px()).floor() as usize
                                        };

                                        if line >= s.lines.len() {
                                            line = s.lines.len() - 1;
                                        }

                                        let mut col = if content_x <= 0.0 {
                                            0
                                        } else {
                                            (content_x / char_px()).floor() as usize
                                        };

                                        let max_col = s.lines[line].len();
                                        if col > max_col {
                                            col = max_col;
                                        }

                                        s.cursor = Cursor { line, col };
                                    });
                                },

                                // caret
                                {
                                    let v = tabs();
                                    let idx = active_tab();
                                    let s = if idx < v.len() { v[idx].editor.clone() } else { EditorState::default() };

                                    let top = (s.cursor.line as f64) * line_px();
                                    let left = (s.cursor.col as f64) * char_px();

                                    rsx!(
                                        div {
                                            class: "caret",
                                            style: "top: calc(var(--pad-y) + {top}px); left: calc(var(--pad-x) + {left}px);"
                                        }
                                    )
                                }

                                // lines (with syntax highlighting)
                                {
                                    let v = tabs();
                                    let idx = active_tab();
                                    let s = if idx < v.len() { v[idx].editor.clone() } else { EditorState::default() };

                                    let total = s.lines.len();
                                    let (start, end, top_h, bottom_h) =
                                        visible_range(scroll_top(), viewport_h(), total);

                                    rsx!(
                                        div { style: "height: {top_h}px;" }
                                        for i in start..end {
                                            {
                                                let line = &s.lines[i];
                                                let spans = crate::syntax::highlight_line(&active_language, line);
                                                rsx!(
                                                    div {
                                                        class: if i == s.cursor.line { "line active" } else { "line" },
                                                        for sp in spans {
                                                            span { style: "color: {sp.color};", "{sp.text}" }
                                                        }
                                                    }
                                                )
                                            }
                                        }
                                        div { style: "height: {bottom_h}px;" }
                                    )
                                }


                            }
                        }
                    }
                }
            }

            // ===== Confirm modal =====
            if confirm_open() {
                div {
                    class: "modal-backdrop",
                    onclick: move |_| {
                        confirm_open.set(false);
                        pending_action.set(PendingAction::None);
                    },

                    div {
                        class: "modal",
                        onclick: move |e| e.stop_propagation(),

                        div { class: "modal-title", "You have unsaved changes." }
                        div {
                            class: "modal-sub",
                            {
                                let what = match pending_action() {
                                    PendingAction::CloseTab(_) => "Close the tab?",
                                    PendingAction::ExitApp => "Exit the app?",
                                    PendingAction::None => "Continue?",
                                };
                                rsx!("Save before continuing? ({what})")
                            }
                        }

                        div { class: "modal-actions",
                            // Cancel
                            button {
                                class: "btn",
                                onclick: move |_| {
                                    confirm_open.set(false);
                                    pending_action.set(PendingAction::None);
                                },
                                "Cancel"
                            }

                            // Discard
                            button {
                                class: "btn btn-danger",
                                onclick: move |_| {
                                    let action = pending_action();
                                    confirm_open.set(false);
                                    pending_action.set(PendingAction::None);

                                    match action {
                                        PendingAction::CloseTab(i) => {
                                            // discard changes and close
                                            close_tab_immediately(tabs.clone(), active_tab.clone(), i);
                                        }
                                        PendingAction::ExitApp => {
                                            dioxus_desktop::window().close();
                                        }
                                        PendingAction::None => {}
                                    }
                                },
                                "Discard"
                            }

                            // Save
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| {
                                    let action = pending_action();
                                    confirm_open.set(false);

                                    let tabs2 = tabs.clone();
                                    let act2 = active_tab.clone();
                                    let mut status2 = status.clone();
                                    let mut pending2 = pending_action.clone();

                                    spawn(async move {
                                        match action.clone() {
                                            PendingAction::CloseTab(i) => {
                                                // Save that tab index (not necessarily active)
                                                // If user cancels save dialog, nothing happens.
                                                let v = tabs2();
                                                if i < v.len() {
                                                    if let Some(p) = v[i].path.clone() {
                                                        save_tab_to_path(tabs2.clone(), i, status2.clone(), p).await;
                                                    } else if let Some(handle) = AsyncFileDialog::new().save_file().await {
                                                        let path = handle.path().to_path_buf();
                                                        save_tab_to_path(tabs2.clone(), i, status2.clone(), path).await;
                                                    }

                                                    // If it saved (dirty cleared), close it.
                                                    let v2 = tabs2();
                                                    if i < v2.len() && !v2[i].dirty {
                                                        close_tab_immediately(tabs2.clone(), act2.clone(), i);
                                                    }
                                                }
                                            }
                                            PendingAction::ExitApp => {
                                                // Save active tab, then exit if clean
                                                let idx = act2();
                                                let v = tabs2();
                                                if idx < v.len() {
                                                    if let Some(p) = v[idx].path.clone() {
                                                        save_tab_to_path(tabs2.clone(), idx, status2.clone(), p).await;
                                                    } else if let Some(handle) = AsyncFileDialog::new().save_file().await {
                                                        let path = handle.path().to_path_buf();
                                                        save_tab_to_path(tabs2.clone(), idx, status2.clone(), path).await;
                                                    }

                                                    if act2() < tabs2().len() && !tabs2()[act2()].dirty {
                                                        dioxus_desktop::window().close();
                                                    }
                                                }
                                            }
                                            PendingAction::None => {}
                                        }

                                        pending2.set(PendingAction::None);
                                    });
                                },
                                "Save"
                            }
                        }
                    }
                }
            }
        }
    }
}

/* ===== EDITING ===== */

fn lines_mut(s: &mut EditorState) -> &mut Vec<String> {
    Arc::make_mut(&mut s.lines)
}

fn handle_key(s: &mut EditorState, key: Key) -> bool {
    match key {
        Key::ArrowLeft => {
            move_left(s);
            false
        }
        Key::ArrowRight => {
            move_right(s);
            false
        }
        Key::ArrowUp => {
            move_up(s);
            false
        }
        Key::ArrowDown => {
            move_down(s);
            false
        }
        Key::Backspace => {
            backspace(s);
            true
        }
        Key::Enter => {
            newline(s);
            true
        }
        Key::Tab => {
            insert_str(s, "    ");
            true
        }
        Key::Character(c) if c.chars().count() == 1 => {
            insert_char(s, c.chars().next().unwrap());
            true
        }
        _ => false,
    }
}

fn insert_char(s: &mut EditorState, ch: char) {
    let Cursor { line, col } = s.cursor;
    let lines = lines_mut(s);
    if line >= lines.len() {
        lines.push(String::new());
    }
    lines[line].insert(col, ch);
    s.cursor.col += ch.len_utf8();
}

fn insert_str(s: &mut EditorState, t: &str) {
    for c in t.chars() {
        insert_char(s, c);
    }
}

fn backspace(s: &mut EditorState) {
    let Cursor { line, col } = s.cursor;
    let lines = lines_mut(s);

    if lines.is_empty() {
        lines.push(String::new());
        s.cursor = Cursor { line: 0, col: 0 };
        return;
    }

    let line = line.min(lines.len().saturating_sub(1));

    if col > 0 {
        if col <= lines[line].len() {
            lines[line].remove(col - 1);
            s.cursor.col = col - 1;
        }
    } else if line > 0 {
        let tail = lines.remove(line);
        let prev = line - 1;
        let len = lines[prev].len();
        lines[prev].push_str(&tail);
        s.cursor = Cursor { line: prev, col: len };
    }
}

fn newline(s: &mut EditorState) {
    let Cursor { line, col } = s.cursor;
    let lines = lines_mut(s);

    if lines.is_empty() {
        lines.push(String::new());
    }

    let line = line.min(lines.len().saturating_sub(1));
    let safe_col = col.min(lines[line].len());
    let rest = lines[line].split_off(safe_col);
    lines.insert(line + 1, rest);
    s.cursor = Cursor { line: line + 1, col: 0 };
}

fn move_left(s: &mut EditorState) {
    if s.cursor.col > 0 {
        s.cursor.col -= 1;
    } else if s.cursor.line > 0 {
        s.cursor.line -= 1;
        s.cursor.col = s.lines[s.cursor.line].len();
    }
}

fn move_right(s: &mut EditorState) {
    if s.cursor.col < s.lines[s.cursor.line].len() {
        s.cursor.col += 1;
    } else if s.cursor.line + 1 < s.lines.len() {
        s.cursor.line += 1;
        s.cursor.col = 0;
    }
}

fn move_up(s: &mut EditorState) {
    if s.cursor.line > 0 {
        s.cursor.line -= 1;
        s.cursor.col = s.cursor.col.min(s.lines[s.cursor.line].len());
    }
}

fn move_down(s: &mut EditorState) {
    if s.cursor.line + 1 < s.lines.len() {
        s.cursor.line += 1;
        s.cursor.col = s.cursor.col.min(s.lines[s.cursor.line].len());
    }
}

fn main() {
    use dioxus::desktop::{Config, LogicalPosition, LogicalSize, WindowBuilder};
    use dioxus::LaunchBuilder;

    let cfg = Config::new()
        .with_menu(None)
        .with_window(
            WindowBuilder::new()
                .with_title("SIDE")
                .with_decorations(true)
                .with_always_on_top(false)
                .with_inner_size(LogicalSize::new(800, 600))
                .with_maximized(true)
                .with_position(LogicalPosition::new(500, 200)),
        );

    LaunchBuilder::desktop().with_cfg(cfg).launch(app);
}
