use include_dir::{include_dir, Dir};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

static SYNTAX_CACHE: Lazy<Mutex<HashMap<String, Syntax>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// Embed syntax definitions into the binary (portable exe).
static SIDEL_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/syntax");

#[derive(Debug, Clone)]
pub struct Syntax {
    pub default_color: String,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Rule {
    pub name: String,
    pub regex: Regex,
    pub color: String,
    pub priority: i32,
}

#[derive(Debug, Clone)]
pub struct HighlightSpan {
    pub text: String,
    pub color: String,
}

#[derive(Debug, Deserialize)]
struct SidelFile {
    #[serde(default = "default_color")]
    default_color: String,
    // IMPORTANT: your .sidel files use [[rule]] (singular)
    #[serde(default)]
    rule: Vec<SidelRule>,
}

#[derive(Debug, Deserialize)]
struct SidelRule {
    #[serde(default)]
    name: String,
    pattern: String,
    color: String,
    #[serde(default = "default_priority")]
    priority: i32,
}

fn default_color() -> String {
    "#D4D4D4".to_string()
}

fn default_priority() -> i32 {
    0
}

pub fn detect_language_from_path(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "rs" => "rust",
        "py" | "pyw" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "html" => "html",
        "css" => "css",
        "json" => "json",
        "md" => "markdown",
        "sidel" => "sidel",
        "c" | "h" => "c",
        "toml" => "toml",
        "cpp" | "hpp" | "hh" | "hxx" | "cc" | "cxx" => "cpp",
        "bf" | "b" => "brainfuck",
        "hc" => "holyc",
        "lol" | "lols" => "lolcode",
        "b93" | "be" | "befunge" => "befunge93",
        "b98" => "befunge98",
        "i" | "3i" | "4i" | "7i" => "intercal",
        "ook" => "ook",
        "chef" => "chef",
        "unl" => "unlambda",
        "arnoldc" => "arnoldc",
        "pygyat" => "pygyat",
        _ => "plain",
    }
    .to_string()
}

fn read_embedded_sidel(language: &str) -> Option<&'static str> {
    let filename = format!("{language}.sidel");
    SIDEL_DIR.get_file(filename)?.contents_utf8()
}

fn disk_sidel_candidates(language: &str) -> Vec<PathBuf> {
    let filename = format!("{language}.sidel");
    let mut v = Vec::new();

    // Best for `cargo run` when CWD is the crate dir.
    v.push(PathBuf::from("syntax").join(&filename));

    // If someone DOES ship a syntax folder next to the exe, allow that too.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.join("syntax").join(&filename));
        }
    }

    // Debug-only absolute path (handy if running from odd working dirs).
    #[cfg(debug_assertions)]
    {
        v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("syntax").join(&filename));
    }

    v
}

/// Load a `.sidel` file:
/// - Optional override via env var SIDE_SYNTAX_DIR
/// - In debug builds, prefer disk for fast iteration
/// - Always fall back to embedded so release builds work anywhere
fn load_sidel_text(language: &str) -> Option<String> {
    // User override (works in debug + release)
    if let Ok(dir) = std::env::var("SIDE_SYNTAX_DIR") {
        let p = PathBuf::from(dir).join(format!("{language}.sidel"));
        if let Ok(s) = fs::read_to_string(p) {
            return Some(s);
        }
    }

    // Dev-mode: prefer reading from disk (edits without recompiling)
    #[cfg(debug_assertions)]
    {
        for p in disk_sidel_candidates(language) {
            if let Ok(s) = fs::read_to_string(&p) {
                return Some(s);
            }
        }
    }

    // Fallback: embedded (portable exe)
    read_embedded_sidel(language).map(|s| s.to_string())
}

pub fn load_syntax(language: &str) -> Syntax {
    if let Some(hit) = SYNTAX_CACHE.lock().unwrap().get(language).cloned() {
        return hit;
    }

    let syntax = match load_sidel_text(language) {
        Some(content) => parse_sidel(&content).unwrap_or_else(|_| fallback_syntax()),
        None => fallback_syntax(),
    };

    SYNTAX_CACHE
        .lock()
        .unwrap()
        .insert(language.to_string(), syntax.clone());

    syntax
}

fn fallback_syntax() -> Syntax {
    Syntax {
        default_color: default_color(),
        rules: vec![],
    }
}

fn parse_sidel(toml_text: &str) -> Result<Syntax, toml::de::Error> {
    let parsed: SidelFile = toml::from_str(toml_text)?;
    let mut rules = Vec::new();

    for r in parsed.rule {
        if let Ok(re) = Regex::new(&r.pattern) {
            rules.push(Rule {
                name: r.name,
                regex: re,
                color: r.color,
                priority: r.priority,
            });
        }
    }

    rules.sort_by(|a, b| b.priority.cmp(&a.priority));

    Ok(Syntax {
        default_color: parsed.default_color,
        rules,
    })
}

pub fn highlight_line(language: &str, line: &str) -> Vec<HighlightSpan> {
    let syn = load_syntax(language);

    let mut spans = vec![HighlightSpan {
        text: line.to_string(),
        color: syn.default_color.clone(),
    }];

    for rule in &syn.rules {
        let mut next = Vec::new();

        for span in spans {
            // Don’t recolor already-colored spans
            if span.color != syn.default_color {
                next.push(span);
                continue;
            }

            let mut last = 0usize;

            for m in rule.regex.find_iter(&span.text) {
                let (s, e) = (m.start(), m.end());

                if s > last {
                    next.push(HighlightSpan {
                        text: span.text[last..s].to_string(),
                        color: syn.default_color.clone(),
                    });
                }

                next.push(HighlightSpan {
                    text: span.text[s..e].to_string(),
                    color: rule.color.clone(),
                });

                last = e;
            }

            if last < span.text.len() {
                next.push(HighlightSpan {
                    text: span.text[last..].to_string(),
                    color: syn.default_color.clone(),
                });
            }
        }

        spans = next;
    }

    spans
}
