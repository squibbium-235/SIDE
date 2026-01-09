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

pub fn syntax_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("syntax")
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
        _ => "plain",
    }
    .to_string()
}

pub fn load_syntax(language: &str) -> Syntax {
    if let Some(hit) = SYNTAX_CACHE.lock().unwrap().get(language).cloned() {
        return hit;
    }

    let path = syntax_dir().join(format!("{language}.sidel"));

    let syntax = match fs::read_to_string(&path) {
        Ok(content) => parse_sidel(&content).unwrap_or_else(|_| fallback_syntax()),
        Err(_) => fallback_syntax(),
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

    if syn.rules.is_empty() || line.is_empty() {
        return vec![HighlightSpan {
            text: line.to_string(),
            color: syn.default_color,
        }];
    }

    let bytes = line.as_bytes();
    let mut color_at: Vec<Option<&str>> = vec![None; bytes.len()];

    for rule in &syn.rules {
        for m in rule.regex.find_iter(line) {
            let start = m.start();
            let end = m.end().min(bytes.len());
            for i in start..end {
                if color_at[i].is_none() {
                    color_at[i] = Some(rule.color.as_str());
                }
            }
        }
    }

    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let cur_color = color_at[i].unwrap_or(syn.default_color.as_str());
        let mut j = i + 1;
        while j < bytes.len() {
            let c = color_at[j].unwrap_or(syn.default_color.as_str());
            if c != cur_color {
                break;
            }
            j += 1;
        }
        spans.push(HighlightSpan {
            text: line[i..j].to_string(),
            color: cur_color.to_string(),
        });
        i = j;
    }

    spans
}
