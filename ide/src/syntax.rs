use include_dir::{include_dir, Dir};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

// Embed the syntax folder (portable exe).
static SIDEL_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/syntax");

// Cache compiled syntax rules
static SYNTAX_CACHE: Lazy<Mutex<HashMap<String, Syntax>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// Load the manifest once
static MANIFEST: Lazy<ManifestData> = Lazy::new(|| load_manifest().unwrap_or_else(|_| ManifestData {
    ext_to_lang: HashMap::new(),
    languages: HashSet::new(),
}));

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

#[derive(Debug, Deserialize)]
struct ManifestFile {
    #[serde(default)]
    language: Vec<ManifestLang>,
}

#[derive(Debug, Deserialize)]
struct ManifestLang {
    name: String,
    #[serde(default)]
    extensions: Vec<String>,
}

struct ManifestData {
    ext_to_lang: HashMap<String, String>,
    languages: HashSet<String>,
}

/// Read embedded file text by name.
fn embedded_text(name: &str) -> Option<&'static str> {
    SIDEL_DIR.get_file(name)?.contents_utf8()
}

/// Disk candidates for debug convenience.
fn disk_candidates(rel_path: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();

    // 1) Relative to current working directory
    v.push(PathBuf::from(rel_path));

    // 2) Relative to the executable folder
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.join(rel_path));
        }
    }

    // 3) Debug-only: absolute path to the crate
    #[cfg(debug_assertions)]
    {
        v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path));
    }

    v
}

fn load_manifest_text() -> Option<String> {
    // Optional override
    if let Ok(dir) = std::env::var("SIDE_SYNTAX_DIR") {
        let p = PathBuf::from(dir).join("manifest.toml");
        if let Ok(s) = fs::read_to_string(p) {
            return Some(s);
        }
    }

    // Debug: prefer disk so you can tweak without recompiling
    #[cfg(debug_assertions)]
    {
        for p in disk_candidates("syntax/manifest.toml") {
            if let Ok(s) = fs::read_to_string(&p) {
                return Some(s);
            }
        }
    }

    // Fallback: embedded
    embedded_text("manifest.toml").map(|s| s.to_string())
}

fn load_manifest() -> Result<ManifestData, String> {
    let text = load_manifest_text().ok_or("manifest.toml not found")?;
    let parsed: ManifestFile =
        toml::from_str(&text).map_err(|e| format!("manifest.toml parse error: {e}"))?;

    let mut ext_to_lang = HashMap::new();
    let mut languages = HashSet::new();

    for lang in parsed.language {
        languages.insert(lang.name.clone());
        for ext in lang.extensions {
            ext_to_lang.insert(ext.to_ascii_lowercase(), lang.name.clone());
        }
    }

    Ok(ManifestData { ext_to_lang, languages })
}

fn read_embedded_sidel(language: &str) -> Option<&'static str> {
    let filename = format!("{language}.sidel");
    SIDEL_DIR.get_file(filename)?.contents_utf8()
}

fn load_sidel_text(language: &str) -> Option<String> {
    // Optional override
    if let Ok(dir) = std::env::var("SIDE_SYNTAX_DIR") {
        let p = PathBuf::from(dir).join(format!("{language}.sidel"));
        if let Ok(s) = fs::read_to_string(p) {
            return Some(s);
        }
    }

    // Debug: prefer disk so edits don't require a rebuild
    #[cfg(debug_assertions)]
    {
        let rel = format!("syntax/{language}.sidel");
        for p in disk_candidates(&rel) {
            if let Ok(s) = fs::read_to_string(&p) {
                return Some(s);
            }
        }
    }

    // Fallback: embedded
    read_embedded_sidel(language).map(|s| s.to_string())
}

pub fn detect_language_from_path(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext.is_empty() {
        return "plain".to_string();
    }

    MANIFEST
        .ext_to_lang
        .get(&ext)
        .cloned()
        .unwrap_or_else(|| "plain".to_string())
}

pub fn load_syntax(language: &str) -> Syntax {
    if let Some(hit) = SYNTAX_CACHE.lock().unwrap().get(language).cloned() {
        return hit;
    }

    // If the manifest doesn't know this language, don't even bother trying.
    if !MANIFEST.languages.contains(language) {
        return fallback_syntax();
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
