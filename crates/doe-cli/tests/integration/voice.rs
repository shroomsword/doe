//! The words a person reads are in the neutral voice: no "I", "we", "my",
//! "our" or "let's" (the software is a tool, not a speaker). This scans the
//! text that reaches a person: the string literals and the doc comments of the
//! Rust sources (up to the tests of each file), the strings of the web page's
//! sources, its HTML, and the documentation. Comments that explain code are not
//! scanned.

use std::fs;
use std::path::{Path, PathBuf};

/// Words that speak in the first person (compared in lower case; `I` alone is
/// looked at separately).
const WORDS: [&str; 17] = [
    "i'm",
    "i'll",
    "i've",
    "i'd",
    "me",
    "my",
    "mine",
    "myself",
    "we",
    "we'll",
    "we're",
    "we've",
    "us",
    "our",
    "ours",
    "ourselves",
    "let's",
];

/// Text that looks like a first-person word and is not: a flag, a Roman
/// numeral, an abbreviation.
const NOT_A_PRONOUN: [&str; 6] = ["-I", "Layer I", "layer I", "I/O", "I/II/III", "I or II"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The first-person words of `text`, with the word before and after for the report.
fn first_person(text: &str) -> Vec<String> {
    let mut cleaned = text.to_owned();
    for allowed in NOT_A_PRONOUN {
        cleaned = cleaned.replace(allowed, " ");
    }
    let tokens: Vec<&str> = cleaned
        .split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’'))
        .filter(|t| !t.is_empty())
        .collect();
    let mut found = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let word = token.replace('’', "'");
        let all_caps = word.len() > 1 && word.chars().all(|c| !c.is_lowercase());
        let hit = if word == "I" {
            // the pronoun: followed by a word in lower case ("I found"), not a numeral
            tokens
                .get(i + 1)
                .is_some_and(|next| next.starts_with(char::is_lowercase))
        } else {
            !all_caps && WORDS.contains(&word.to_lowercase().as_str())
        };
        if hit {
            let from = i.saturating_sub(2);
            let to = (i + 3).min(tokens.len());
            found.push(tokens[from..to].join(" "));
        }
    }
    found
}

/// The string literals and the doc comments (`///`, `//!`) of Rust source, up
/// to the first `#[cfg(test)]`.
fn rust_text(source: &str) -> String {
    let source = source.split("#[cfg(test)]").next().unwrap_or("");
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            let doc = matches!(chars.get(i + 2), Some('/' | '!'));
            while i < chars.len() && chars[i] != '\n' {
                if doc {
                    out.push(chars[i]);
                }
                i += 1;
            }
            out.push('\n');
        } else if c == '/' && next == Some('*') {
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c == '\'' {
            // a character literal ('"', '\n') or a lifetime
            if chars.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < chars.len() && chars[i] != '\'' {
                    i += 1;
                }
            } else if chars.get(i + 2) == Some(&'\'') {
                i += 2;
            }
            i += 1;
        } else if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    i += 1;
                }
                if let Some(&k) = chars.get(i) {
                    out.push(k);
                }
                i += 1;
            }
            out.push('\n');
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

/// The string literals of TypeScript or JavaScript source, comments left out.
fn script_text(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c == '\'' || c == '"' || c == '`' {
            let quote = c;
            i += 1;
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\\' {
                    i += 1;
                }
                if let Some(&k) = chars.get(i) {
                    out.push(k);
                }
                i += 1;
            }
            out.push('\n');
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

/// HTML without its scripts and styles.
fn html_text(source: &str) -> String {
    let mut out = String::new();
    let mut rest = source;
    loop {
        let next = ["<script", "<style"]
            .iter()
            .filter_map(|tag| rest.find(tag).map(|at| (at, *tag)))
            .min();
        let Some((at, tag)) = next else {
            out.push_str(rest);
            break;
        };
        let (before, after) = rest.split_at(at);
        out.push_str(before);
        let close = format!("</{}>", tag.trim_start_matches('<'));
        match after.split_once(close.as_str()) {
            Some((_, tail)) => rest = tail,
            None => break,
        }
    }
    out
}

fn files_below(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_below(&path, extension, out);
        } else if path.extension().is_some_and(|e| e == extension) {
            out.push(path);
        }
    }
}

#[test]
fn what_a_person_reads_does_not_speak_in_the_first_person() {
    let root = root();
    let mut problems = Vec::new();
    let mut check = |path: &Path, text: &str| {
        for context in first_person(text) {
            let name = path.strip_prefix(&root).unwrap_or(path);
            problems.push(format!("{}: …{context}…", name.display()));
        }
    };

    let mut rust = Vec::new();
    for krate in ["doe", "doe-cli", "doe-tui", "doe-wasm"] {
        files_below(
            &root.join("crates").join(krate).join("src"),
            "rs",
            &mut rust,
        );
    }
    for path in &rust {
        check(path, &rust_text(&fs::read_to_string(path).unwrap()));
    }

    let mut scripts = Vec::new();
    files_below(&root.join("web").join("src"), "ts", &mut scripts);
    scripts.push(root.join("web").join("build.mjs"));
    for path in scripts
        .iter()
        .filter(|p| !p.to_string_lossy().contains("generated"))
    {
        check(path, &script_text(&fs::read_to_string(path).unwrap()));
    }

    let page = root.join("web").join("index.html");
    check(&page, &html_text(&fs::read_to_string(&page).unwrap()));

    for doc in ["README.md", "web/README.md", "types/README.md"] {
        let path = root.join(doc);
        if let Ok(text) = fs::read_to_string(&path) {
            check(&path, &text);
        }
    }
    let mut types = Vec::new();
    files_below(&root.join("types"), "yaml", &mut types);
    for path in &types {
        check(path, &fs::read_to_string(path).unwrap());
    }

    assert!(
        problems.is_empty(),
        "first person in what a person reads:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_check_finds_the_pronouns_and_leaves_flags_and_numerals() {
    assert_eq!(first_person("I found no types"), ["I found no"]);
    assert_eq!(first_person("could not find it, we give up").len(), 1);
    assert_eq!(first_person("Let's try our best; my file").len(), 3);
    assert_eq!(first_person("it's mine").len(), 1);
    assert_eq!(
        first_person("give one with -I; layer I or II; I/O; US and ME"),
        Vec::<String>::new()
    );
    assert_eq!(first_person("a type with an id"), Vec::<String>::new());
}

#[test]
fn the_text_of_rust_source_is_its_strings_and_docs_without_the_tests() {
    let source = "/// the doc: we\nfn f() { let a = \"my \\\" text\"; // we note\n let b = '\"'; let c = \"fine\"; }\n#[cfg(test)]\nmod t { fn g() { \"our\"; } }\n";
    let text = rust_text(source);
    assert!(text.contains("the doc: we"));
    assert!(text.contains("my \" text"));
    assert!(text.contains("fine"));
    assert!(!text.contains("we note"));
    assert!(!text.contains("our"));
}
