//! The languages of the preview of a file of an archive: which one a file is in, from its name and, when the name does
//! not say, from a `#!` first line, and the name of the grammar of each (the grammars that come with `syntect`).
//!
//! The web IDE has a table of its own (`web/src/languages.ts`) with the same ids and the same extensions: a test of the
//! web IDE reads this file and compares the two. A language that the grammars here do not have (`toml`, `ini`,
//! `dockerfile`) is told, and its files are shown plain.

/// The grammar of each language, by the id the web IDE also uses; `None`: the grammars have none, and the file is plain.
pub const LANGUAGES: &[(&str, &str, Option<&str>)] = &[
    ("c", "C", Some("C")),
    ("cpp", "C++", Some("C++")),
    ("css", "CSS", Some("CSS")),
    ("diff", "Diff", Some("Diff")),
    ("dockerfile", "Dockerfile", None),
    ("html", "HTML", Some("HTML")),
    ("ini", "INI", None),
    ("javascript", "JavaScript", Some("JavaScript")),
    ("json", "JSON", Some("JSON")),
    ("markdown", "Markdown", Some("Markdown")),
    ("python", "Python", Some("Python")),
    ("rust", "Rust", Some("Rust")),
    ("shell", "Shell", Some("Bourne Again Shell (bash)")),
    ("sql", "SQL", Some("SQL")),
    ("toml", "TOML", None),
    // the grammars have no TypeScript; its keywords and strings are those of JavaScript
    ("typescript", "TypeScript", Some("JavaScript")),
    ("xml", "XML", Some("XML")),
    ("yaml", "YAML", Some("YAML")),
];

/// The language of each extension (lower case, without the dot).
pub const EXTENSIONS: &[(&str, &str)] = &[
    ("c", "c"),
    ("h", "c"),
    ("cc", "cpp"),
    ("cpp", "cpp"),
    ("cxx", "cpp"),
    ("hh", "cpp"),
    ("hpp", "cpp"),
    ("hxx", "cpp"),
    ("css", "css"),
    ("diff", "diff"),
    ("patch", "diff"),
    ("htm", "html"),
    ("html", "html"),
    ("xhtml", "html"),
    ("cfg", "ini"),
    ("conf", "ini"),
    ("ini", "ini"),
    ("cjs", "javascript"),
    ("js", "javascript"),
    ("jsx", "javascript"),
    ("mjs", "javascript"),
    ("json", "json"),
    ("geojson", "json"),
    ("md", "markdown"),
    ("markdown", "markdown"),
    ("py", "python"),
    ("pyi", "python"),
    ("rs", "rust"),
    ("bash", "shell"),
    ("sh", "shell"),
    ("zsh", "shell"),
    ("sql", "sql"),
    ("toml", "toml"),
    ("cts", "typescript"),
    ("mts", "typescript"),
    ("ts", "typescript"),
    ("tsx", "typescript"),
    ("plist", "xml"),
    ("svg", "xml"),
    ("xml", "xml"),
    ("xsd", "xml"),
    ("xsl", "xml"),
    ("ksy", "yaml"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
];

/// The language of some whole file names (lower case).
pub const NAMES: &[(&str, &str)] = &[
    (".bashrc", "shell"),
    (".profile", "shell"),
    (".zshrc", "shell"),
    ("dockerfile", "dockerfile"),
    ("containerfile", "dockerfile"),
    ("gnumakefile", "shell"),
    ("makefile", "shell"),
    ("pkgbuild", "shell"),
    ("cargo.lock", "toml"),
    ("meson.build", "python"),
    ("sconstruct", "python"),
];

/// The name of the language of an id.
#[must_use]
pub fn name(id: &str) -> Option<&'static str> {
    LANGUAGES
        .iter()
        .find(|(known, _, _)| *known == id)
        .map(|(_, name, _)| *name)
}

/// The name of the `syntect` grammar of a language, if the grammars have one.
#[must_use]
pub fn grammar(id: &str) -> Option<&'static str> {
    LANGUAGES
        .iter()
        .find(|(known, _, _)| *known == id)
        .and_then(|(_, _, grammar)| *grammar)
}

/// The language a `#!` first line of `text` names: `#!/bin/sh`, `#!/usr/bin/env python3`,
/// `#!/usr/bin/env -S node --flag`.
#[must_use]
pub fn from_shebang(text: &str) -> Option<&'static str> {
    let line = text.strip_prefix("#!")?.lines().next()?;
    let mut words = line.split_whitespace();
    let mut program = words.next().unwrap_or("");
    if program.rsplit('/').next() == Some("env") {
        // `env` takes options and NAME=value words before the program
        program = words
            .find(|word| !word.starts_with('-') && !word.contains('='))
            .unwrap_or("");
    }
    let base = program.rsplit('/').next().unwrap_or("");
    match base {
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "ash" => Some("shell"),
        "node" | "nodejs" | "deno" | "bun" => Some("javascript"),
        _ if base
            .strip_prefix("python")
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit() || c == '.')) =>
        {
            Some("python")
        }
        _ => None,
    }
}

/// The language of a file from its name (the last part of its path) and, when the name does not say, from a `#!` first
/// line of `text`. `None` when neither tells: the file stays plain.
#[must_use]
pub fn detect(name: &str, text: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if let Some((_, id)) = NAMES.iter().find(|(known, _)| *known == lower) {
        return Some(id);
    }
    if lower.starts_with("dockerfile.") || lower.ends_with(".dockerfile") {
        return Some("dockerfile");
    }
    if let Some((stem, extension)) = lower.rsplit_once('.')
        && !stem.is_empty()
        && let Some((_, id)) = EXTENSIONS.iter().find(|(known, _)| *known == extension)
    {
        return Some(id);
    }
    from_shebang(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_language_comes_from_the_name_the_file_name_or_a_shebang() {
        assert_eq!(detect("main.rs", ""), Some("rust"));
        assert_eq!(detect("Main.PY", ""), Some("python"));
        assert_eq!(detect("a.tar.gz.json", ""), Some("json"));
        assert_eq!(detect("README.md", ""), Some("markdown"));
        assert_eq!(detect("Makefile", ""), Some("shell"));
        assert_eq!(detect("Dockerfile", ""), Some("dockerfile"));
        assert_eq!(detect("Dockerfile.dev", ""), Some("dockerfile"));
        assert_eq!(detect(".bashrc", ""), Some("shell"));
        assert_eq!(detect("run", "#!/bin/sh\necho hi\n"), Some("shell"));
        assert_eq!(
            detect("run", "#!/usr/bin/env python3\nprint(1)\n"),
            Some("python")
        );
        assert_eq!(
            detect("run", "#!/usr/bin/env -S node --flag\n"),
            Some("javascript")
        );
        // the name wins over the line
        assert_eq!(detect("x.rs", "#!/bin/sh\n"), Some("rust"));
        assert_eq!(detect("notes.xyz", "just words\n"), None);
        assert_eq!(detect("LICENSE", "Permission is hereby granted\n"), None);
        assert_eq!(from_shebang("#!/usr/bin/perl\n"), None);
        assert_eq!(from_shebang("# not a shebang\n"), None);
    }

    #[test]
    fn every_language_of_the_tables_is_in_the_list() {
        for (_, id) in EXTENSIONS.iter().chain(NAMES) {
            assert!(name(id).is_some(), "{id} is not in LANGUAGES");
        }
    }
}
