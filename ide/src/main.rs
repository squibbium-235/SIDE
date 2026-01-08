use base64::{engine::general_purpose::STANDARD, Engine as _};
use dioxus::prelude::*;
use rfd::AsyncFileDialog;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default)]
struct Cursor {
    line: usize,
    col: usize,
}

#[derive(Clone, Debug)]
struct EditorState {
    lines: Vec<String>,
    cursor: Cursor,
    scroll_x: f64,
    scroll_y: f64,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: Cursor::default(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingAction {
    None,
    NewFile,
    OpenFile,
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

fn join_lines(lines: &[String]) -> String {
    lines.join("\n")
}

fn split_lines(text: &str) -> Vec<String> {
    let mut v: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    if v.is_empty() {
        v.push(String::new());
    }
    v
}

/// Build CSS + bundled font
/// Place JetBrainsMono-Regular.ttf at: assets/fonts/JetBrainsMono-Regular.ttf
fn bundled_css() -> String {
    const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
    let b64 = STANDARD.encode(FONT_BYTES);

    // Use a raw template string; avoid `format!` because CSS uses braces.
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
  min-width: 190px;
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

/* ===== EDITOR LAYOUT ===== */
.editor-wrap {
  flex: 1;
  min-height: 0;
}

.scroll {
  width: 100%;
  height: 100%;
  overflow: auto;
  outline: none;
}

.row {
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
  min-width: 700px;
}

/* Make clicks hit .textpane, not child .line divs */
.line {
  height: var(--line-h);
  pointer-events: none;
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
"#;

    template
        .replace("__B64__", &b64)
        .replace("__PAD_X__", &format!("{PAD_X_PX}"))
        .replace("__PAD_Y__", &format!("{PAD_Y_PX}"))
        .replace("__LINE_PX__", &format!("{}", line_px()))
        .replace("__FONT_PX__", &format!("{FONT_PX}"))
        .replace("__LINE_PX__", &format!("{}", line_px()))
}


/// Reset to a blank buffer (like "New File")
fn do_new(mut st: Signal<EditorState>) {
    let mut s = st();
    s.lines = vec![String::new()];
    s.cursor = Cursor { line: 0, col: 0 };
    s.scroll_x = 0.0;
    s.scroll_y = 0.0;
    st.set(s);
}

async fn open_dialog_and_load(
    mut st: Signal<EditorState>,
    mut current_path: Signal<Option<PathBuf>>,
    mut dirty: Signal<bool>,
    mut status: Signal<String>,
) {
    if let Some(handle) = AsyncFileDialog::new().pick_file().await {
        let path = handle.path().to_path_buf();
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                let mut s = st();
                s.lines = split_lines(&contents);
                s.cursor = Cursor { line: 0, col: 0 };
                st.set(s);

                current_path.set(Some(path.clone()));
                dirty.set(false);
                status.set(format!("Opened {}", path.display()));
            }
            Err(err) => status.set(format!("Open failed: {}", err)),
        }
    }
}

async fn save_to_path(
    st: Signal<EditorState>,
    mut current_path: Signal<Option<PathBuf>>,
    mut dirty: Signal<bool>,
    mut status: Signal<String>,
    path: PathBuf,
) {
    let text = join_lines(&st().lines);
    match std::fs::write(&path, text) {
        Ok(()) => {
            current_path.set(Some(path.clone()));
            dirty.set(false);
            status.set(format!("Saved {}", path.display()));
        }
        Err(err) => status.set(format!("Save failed: {}", err)),
    }
}

async fn save_or_save_as(
    st: Signal<EditorState>,
    current_path: Signal<Option<PathBuf>>,
    dirty: Signal<bool>,
    status: Signal<String>,
) {
    if let Some(p) = current_path() {
        save_to_path(st, current_path, dirty, status, p).await;
        return;
    }

    if let Some(handle) = AsyncFileDialog::new().save_file().await {
        let path = handle.path().to_path_buf();
        save_to_path(st, current_path, dirty, status, path).await;
    }
}

async fn save_as(
    st: Signal<EditorState>,
    current_path: Signal<Option<PathBuf>>,
    dirty: Signal<bool>,
    status: Signal<String>,
) {
    if let Some(handle) = AsyncFileDialog::new().save_file().await {
        let path = handle.path().to_path_buf();
        save_to_path(st, current_path, dirty, status, path).await;
    }
}

pub fn app() -> Element {
    let css = bundled_css();

    // NOTE: in this Dioxus version, Signal::set needs &mut self, so these bindings must be mutable.
    let mut st = use_signal(EditorState::default);

    let mut file_open = use_signal(|| false);
    let mut status = use_signal(|| "".to_string());

    let mut current_path = use_signal(|| Option::<PathBuf>::None);
    let mut dirty = use_signal(|| false);

    let mut confirm_open = use_signal(|| false);
    let mut pending_action = use_signal(|| PendingAction::None);

    let file_label = {
        let name = current_path()
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let star = if dirty() { "*" } else { "" };
        format!("{name}{star}")
    };

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

                            // New
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);

                                    if dirty() {
                                        pending_action.set(PendingAction::NewFile);
                                        confirm_open.set(true);
                                        return;
                                    }

                                    do_new(st.clone());
                                    current_path.set(None);
                                    dirty.set(false);
                                    status.set("New file".to_string());
                                },
                                "New"
                            }

                            // Open…
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);

                                    if dirty() {
                                        pending_action.set(PendingAction::OpenFile);
                                        confirm_open.set(true);
                                        return;
                                    }

                                    let st2 = st.clone();
                                    let cp2 = current_path.clone();
                                    let dirty2 = dirty.clone();
                                    let status2 = status.clone();
                                    spawn(async move { open_dialog_and_load(st2, cp2, dirty2, status2).await; });
                                },
                                "Open"
                            }

                            // Save
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);

                                    let st2 = st.clone();
                                    let cp2 = current_path.clone();
                                    let dirty2 = dirty.clone();
                                    let status2 = status.clone();
                                    spawn(async move { save_or_save_as(st2, cp2, dirty2, status2).await; });
                                },
                                "Save"
                            }

                            // Save As…
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);

                                    let st2 = st.clone();
                                    let cp2 = current_path.clone();
                                    let dirty2 = dirty.clone();
                                    let status2 = status.clone();
                                    spawn(async move { save_as(st2, cp2, dirty2, status2).await; });
                                },
                                "Save As"
                            }

                            div { class: "menu-sep" }

                            // Exit
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);

                                    if dirty() {
                                        pending_action.set(PendingAction::ExitApp);
                                        confirm_open.set(true);
                                        return;
                                    }

                                    // graceful close (desktop)
                                    dioxus_desktop::window().close();
                                },
                                "Exit"
                            }
                        }
                    }
                }

                div { class: "file-indicator", "{file_label}" }
                div { class: "file-indicator", "{status()}" }
            }

            // ===== Editor =====
            div { class: "editor-wrap",
                div {
                    class: "scroll",
                    tabindex: "0",

                    onscroll: move |e| {
                        let mut s = st();
                        s.scroll_y = e.data().scroll_top();
                        s.scroll_x = e.data().scroll_left();
                        st.set(s);
                    },

                    onkeydown: move |e| {
                        let mut s = st();
                        let changed = handle_key(&mut s, e.data().key());
                        st.set(s);

                        if changed {
                            dirty.set(true);
                        }

                        e.prevent_default();
                        e.stop_propagation();
                    },

                    div { class: "row",
                        // gutter
                        div { class: "gutter",
                            for (i, _) in st().lines.iter().enumerate() {
                                div {
                                    class: if i == st().cursor.line { "ln active" } else { "ln" },
                                    "{i + 1}"
                                }
                            }
                        }

                        // text pane
                        div {
                            class: "textpane",

                            onclick: move |e| {
                                let mut s = st();
                                let p = e.data().coordinates().element();

                                // IMPORTANT: element() coords already behave "local enough" in your build.
                                // Adding scroll here double-counts it, which is why clicks jump to the last line.
                                let content_x = (p.x - PAD_X_PX) + CLICK_COL_BIAS_PX;
                                let content_y =  p.y - PAD_Y_PX;

                                if s.lines.is_empty() {
                                    s.lines.push(String::new());
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
                                st.set(s);
                            },


                            // caret
                            {
                                let s = st();
                                let top = (s.cursor.line as f64) * line_px();
                                let left = (s.cursor.col as f64) * char_px();

                                rsx!(
                                    div {
                                        class: "caret",
                                        style: "top: calc(var(--pad-y) + {top}px); left: calc(var(--pad-x) + {left}px);"
                                    }
                                )
                            }

                            // lines
                            for (i, line) in st().lines.iter().enumerate() {
                                div {
                                    class: if i == st().cursor.line { "line active" } else { "line" },
                                    "{line}"
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
                        div { class: "modal-sub", "Save before continuing?" }

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
                                    confirm_open.set(false);
                                    dirty.set(false);

                                    match pending_action() {
                                        PendingAction::NewFile => {
                                            do_new(st.clone());
                                            current_path.set(None);
                                            dirty.set(false);
                                            status.set("New file".to_string());
                                        }
                                        PendingAction::OpenFile => {
                                            let st2 = st.clone();
                                            let cp2 = current_path.clone();
                                            let dirty2 = dirty.clone();
                                            let status2 = status.clone();
                                            spawn(async move { open_dialog_and_load(st2, cp2, dirty2, status2).await; });
                                        }
                                        PendingAction::ExitApp => {
                                            dioxus_desktop::window().close();
                                        }
                                        PendingAction::None => {}
                                    }

                                    pending_action.set(PendingAction::None);
                                },
                                "Discard"
                            }

                            // Save
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| {
                                    confirm_open.set(false);

                                    let st2 = st.clone();
                                    let mut cp2 = current_path.clone();
                                    let mut dirty2 = dirty.clone();
                                    let mut status2 = status.clone();
                                    let mut pending2 = pending_action.clone();

                                    spawn(async move {
                                        save_or_save_as(st2.clone(), cp2.clone(), dirty2.clone(), status2.clone()).await;

                                        if !dirty2() {
                                            match pending2() {
                                                PendingAction::NewFile => {
                                                    do_new(st2.clone());
                                                    cp2.set(None);
                                                    dirty2.set(false);
                                                    status2.set("New file".to_string());
                                                }
                                                PendingAction::OpenFile => {
                                                    open_dialog_and_load(st2, cp2, dirty2, status2).await;
                                                }
                                                PendingAction::ExitApp => {
                                                    dioxus_desktop::window().close();
                                                }
                                                PendingAction::None => {}
                                            }
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
    s.lines[line].insert(col, ch);
    s.cursor.col += ch.len_utf8();
}

fn insert_str(s: &mut EditorState, t: &str) {
    for c in t.chars() {
        insert_char(s, c);
    }
}

fn backspace(s: &mut EditorState) {
    let Cursor { line, col } = s.cursor;
    if col > 0 {
        s.lines[line].remove(col - 1);
        s.cursor.col -= 1;
    } else if line > 0 {
        let tail = s.lines.remove(line);
        let prev = line - 1;
        let len = s.lines[prev].len();
        s.lines[prev].push_str(&tail);
        s.cursor = Cursor { line: prev, col: len };
    }
}

fn newline(s: &mut EditorState) {
    let Cursor { line, col } = s.cursor;
    let rest = s.lines[line].split_off(col);
    s.lines.insert(line + 1, rest);
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
    use dioxus::desktop::{Config, WindowBuilder};
    use dioxus::LaunchBuilder;

    let cfg = Config::new()
        .with_menu(None) // removes the native "Window / Edit / Help" menu bar
        .with_window(
            WindowBuilder::new()
                .with_title("IDE")
                .with_decorations(true)      // keep titlebar + min/max/close
                .with_always_on_top(false),  // optional
        );

    LaunchBuilder::desktop()
        .with_cfg(cfg)
        .launch(app);
}