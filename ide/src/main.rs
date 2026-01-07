use base64::{engine::general_purpose::STANDARD, Engine as _};
use dioxus::prelude::*;
use rfd::FileDialog;
use std::{fs, path::PathBuf};

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

/* ===== METRICS ===== */
const FONT_PX: f64 = 14.0;
const LINE_HEIGHT_EM: f64 = 1.4;
const PAD_X_PX: f64 = 10.0;
const PAD_Y_PX: f64 = 8.0;
const CHAR_WIDTH_RATIO: f64 = 0.60;

// “Forgiveness” so clicks slightly left still land on the intended column
const CLICK_COL_BIAS_PX: f64 = 2.0;

fn line_px() -> f64 {
    FONT_PX * LINE_HEIGHT_EM
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

fn bundled_css() -> String {
    // Bundle the font into the binary
    const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
    let b64 = STANDARD.encode(FONT_BYTES);

    // Raw CSS template (NOT format! so braces don't explode)
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
  --line-h: __LINE_H__em;
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
  min-width: 180px;
  background: #0c0f16;
  border: 1px solid var(--border);
  box-shadow: 0 8px 30px rgba(0,0,0,0.35);
  padding: 6px;
  z-index: 999;
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
  line-height: __LINE_H__;
  min-width: 700px;
}

/* Critical: make clicks hit .textpane, not child .line divs */
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
"#;

    template
        .replace("__B64__", &b64)
        .replace("__PAD_X__", &format!("{PAD_X_PX}"))
        .replace("__PAD_Y__", &format!("{PAD_Y_PX}"))
        .replace("__LINE_H__", &format!("{LINE_HEIGHT_EM}"))
        .replace("__FONT_PX__", &format!("{FONT_PX}"))
}

pub fn app() -> Element {
    let mut st = use_signal(EditorState::default);
    let css = bundled_css();

    let mut file_open = use_signal(|| false);
    let mut status = use_signal(|| "".to_string());
    let mut current_path = use_signal(|| Option::<PathBuf>::None);

    rsx! {
        style { "{css}" }

        div {
            class: "app",

            // Click outside closes dropdown
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

                            // Open…
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);

                                    if let Some(path) = FileDialog::new()
                                        .add_filter("Text", &["txt", "rs", "toml", "md"])
                                        .pick_file()
                                    {
                                        match fs::read_to_string(&path) {
                                            Ok(contents) => {
                                                let mut s = st();
                                                s.lines = split_lines(&contents);
                                                s.cursor = Cursor { line: 0, col: 0 };
                                                st.set(s);

                                                current_path.set(Some(path.clone()));
                                                status.set(format!("Opened {}", path.display()));
                                            }
                                            Err(err) => status.set(format!("Open failed: {}", err)),
                                        }
                                    }
                                },
                                "Open…"
                            }

                            // Save…
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);

                                    if let Some(path) = current_path() {
                                        let text = join_lines(&st().lines);
                                        match fs::write(&path, text) {
                                            Ok(()) => status.set(format!("Saved {}", path.display())),
                                            Err(err) => status.set(format!("Save failed: {}", err)),
                                        }
                                    } else if let Some(path) = FileDialog::new()
                                        .add_filter("Text", &["txt", "rs", "toml", "md"])
                                        .save_file()
                                    {
                                        let text = join_lines(&st().lines);
                                        match fs::write(&path, text) {
                                            Ok(()) => {
                                                current_path.set(Some(path.clone()));
                                                status.set(format!("Saved {}", path.display()));
                                            }
                                            Err(err) => status.set(format!("Save failed: {}", err)),
                                        }
                                    }
                                },
                                "Save"
                            }

                            // Save As…
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    file_open.set(false);

                                    if let Some(path) = FileDialog::new()
                                        .add_filter("Text", &["txt", "rs", "toml", "md"])
                                        .save_file()
                                    {
                                        let text = join_lines(&st().lines);
                                        match fs::write(&path, text) {
                                            Ok(()) => {
                                                current_path.set(Some(path.clone()));
                                                status.set(format!("Saved {}", path.display()));
                                            }
                                            Err(err) => status.set(format!("Save failed: {}", err)),
                                        }
                                    }
                                },
                                "Save As…"
                            }

                            div { class: "menu-sep" }

                            // Exit
                            button {
                                class: "menu-item",
                                onclick: move |_| {
                                    std::process::exit(0);
                                    ()
                                },
                                "Exit"
                            }
                        }
                    }
                }

                div { style: "margin-left: 12px; color: var(--muted); font-size: 12px;",
                    "{status()}"
                }
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
                        handle_key(&mut s, e.data().key());
                        st.set(s);

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

                                let content_x = (p.x + s.scroll_x - PAD_X_PX) + CLICK_COL_BIAS_PX;
                                let content_y =  p.y + s.scroll_y - PAD_Y_PX;

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
        }
    }
}

/* ===== EDITING ===== */

fn handle_key(s: &mut EditorState, key: Key) {
    match key {
        Key::ArrowLeft => move_left(s),
        Key::ArrowRight => move_right(s),
        Key::ArrowUp => move_up(s),
        Key::ArrowDown => move_down(s),
        Key::Backspace => backspace(s),
        Key::Enter => newline(s),
        Key::Tab => insert_str(s, "    "),
        Key::Character(c) if c.chars().count() == 1 => insert_char(s, c.chars().next().unwrap()),
        _ => {}
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
    dioxus::launch(app);
}
