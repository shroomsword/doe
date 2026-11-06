//! Syntax highlighting of the text of a file of an archive, for the preview.
//!
//! The grammars are those that come with `syntect`, run with its regex engine in Rust. Its colour themes are not used:
//! what a grammar says each piece is (`comment`, `string.quoted`, `keyword.control`...) is mapped onto the few kinds of
//! [`Token`] that the viewer's own theme has a colour for (see [`crate::ui::Theme::code`]), as the web IDE maps the
//! Lezer tags onto the page's theme. Text that a grammar does not understand is left plain, never changed or lost.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use syntect::easy::ScopeRangeIterator;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

/// What a piece of code is, by the colour it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    /// A comment.
    Comment,
    /// A string, a character, and the text of a block of raw text.
    String,
    /// A number.
    Number,
    /// A keyword, or a storage class: `fn`, `if`, `class`, `const`; a tag of markup; a heading.
    Keyword,
    /// The name of a type or a class; the name of an attribute of markup; a link.
    Type,
    /// The name of a function.
    Function,
    /// `true`, `null` and other constants of a language.
    Constant,
    /// An operator or punctuation.
    Punct,
    /// Something wrong, or deleted (a line of a diff that goes).
    Invalid,
}

/// The pieces of one line that are not plain, in order, as byte ranges.
pub type Marks = Vec<(Range<usize>, Token)>;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// What one scope of a grammar is, or `None` for a scope that says nothing about the colour (look at the one around).
fn classify(scope: &str) -> Option<Token> {
    // the quote or the `//` of a string or a comment is the colour of the string or the comment
    if scope.starts_with("punctuation.definition.string")
        || scope.starts_with("punctuation.definition.comment")
    {
        return None;
    }
    let starts = |prefix: &str| scope.starts_with(prefix);
    Some(if starts("comment") {
        Token::Comment
    } else if starts("string") || starts("markup.raw") || starts("markup.inserted") {
        Token::String
    } else if starts("constant.numeric") {
        Token::Number
    } else if starts("constant") {
        Token::Constant
    } else if starts("keyword.operator") || starts("punctuation") {
        Token::Punct
    } else if starts("keyword")
        || starts("storage")
        || starts("entity.name.tag")
        || starts("markup.heading")
        || starts("markup.bold")
    {
        Token::Keyword
    } else if starts("entity.name.function")
        || starts("support.function")
        || starts("entity.name.section")
    {
        Token::Function
    } else if starts("entity.name")
        || starts("entity.other")
        || starts("support.type")
        || starts("support.class")
        || starts("markup.underline.link")
        || starts("markup.italic")
    {
        Token::Type
    } else if starts("invalid") || starts("markup.deleted") {
        Token::Invalid
    } else {
        return None;
    })
}

/// The colour of the innermost scope of the stack that has one.
fn token_of(stack: &ScopeStack, cache: &mut HashMap<Scope, Option<Token>>) -> Option<Token> {
    stack.as_slice().iter().rev().find_map(|scope| {
        *cache
            .entry(*scope)
            .or_insert_with(|| classify(&scope.build_string()))
    })
}

/// The pieces of each of `lines` (text without line ends) in the grammar of the language `id` of
/// [`crate::languages`]; `None` if the grammars have none for it. A line that the grammar cannot read (its regex engine
/// gave up) ends the colours: the lines from it on are plain.
#[must_use]
pub fn highlight(id: &str, lines: &[String]) -> Option<Vec<Marks>> {
    let set = syntaxes();
    let syntax = set.find_syntax_by_name(crate::languages::grammar(id)?)?;
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut cache = HashMap::new();
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let terminated = format!("{line}\n");
        let Ok(ops) = state.parse_line(&terminated, set) else {
            break;
        };
        let mut marks = Marks::new();
        for (range, op) in ScopeRangeIterator::new(&ops, &terminated) {
            if stack.apply(op).is_err() {
                break;
            }
            let end = range.end.min(line.len());
            if range.start >= end {
                continue;
            }
            let Some(token) = token_of(&stack, &mut cache) else {
                continue;
            };
            match marks.last_mut() {
                Some((last, kind)) if *kind == token && last.end == range.start => last.end = end,
                _ => marks.push((range.start..end, token)),
            }
        }
        out.push(marks);
    }
    // the lines the grammar did not get to
    out.resize(lines.len(), Marks::new());
    Some(out)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::string_slice,
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_owned).collect()
    }

    /// The text under each mark of a line, with its token.
    fn pieces<'a>(line: &'a str, marks: &[(Range<usize>, Token)]) -> Vec<(&'a str, Token)> {
        marks
            .iter()
            .map(|(range, token)| (&line[range.clone()], *token))
            .collect()
    }

    #[test]
    fn rust_has_comments_strings_numbers_and_keywords() {
        let text =
            lines("fn main() {\n    // say hello\n    let n = 42;\n    println!(\"hello\");\n}");
        let marks = highlight("rust", &text).expect("a grammar for Rust");
        assert_eq!(marks.len(), text.len());
        assert!(pieces(&text[0], &marks[0]).contains(&("fn", Token::Keyword)));
        assert_eq!(
            pieces(&text[1], &marks[1])[0],
            ("// say hello", Token::Comment)
        );
        assert!(pieces(&text[2], &marks[2]).contains(&("42", Token::Number)));
        assert!(pieces(&text[3], &marks[3]).contains(&("\"hello\"", Token::String)));
    }

    #[test]
    fn python_json_shell_and_markdown_have_grammars() {
        for (id, text, wanted) in [
            ("python", "x = 'a' # note", Token::Comment),
            ("json", "{\"n\": 42}", Token::Number),
            ("shell", "echo \"hi\" # note", Token::Comment),
            ("markdown", "# Title", Token::Keyword),
            ("c", "int x = 1; // c", Token::Comment),
            ("yaml", "a: 1 # c", Token::Comment),
            ("css", "a { color: red; } /* c */", Token::Comment),
            ("html", "<a href=\"x\">y</a>", Token::String),
            ("sql", "select 1 -- c", Token::Comment),
            ("typescript", "const x: number = 1; // c", Token::Comment),
        ] {
            let text = lines(text);
            let marks = highlight(id, &text).unwrap_or_else(|| panic!("a grammar for {id}"));
            assert!(
                marks[0].iter().any(|(_, token)| *token == wanted),
                "{id}: {:?}",
                pieces(&text[0], &marks[0])
            );
        }
    }

    #[test]
    fn a_language_with_no_grammar_gives_none_and_a_block_comment_goes_on_to_the_next_line() {
        assert!(highlight("toml", &lines("a = 1")).is_none());
        assert!(highlight("nothing", &lines("a")).is_none());
        let text = lines("/* a\nb */ int x;");
        let marks = highlight("c", &text).expect("a grammar for C");
        assert_eq!(pieces(&text[1], &marks[1])[0], ("b */", Token::Comment));
    }

    #[test]
    fn the_marks_are_inside_the_line_and_in_order() {
        let text = lines("let s = \"a é b\"; // ü\nfn x() {}");
        for id in ["rust", "javascript"] {
            let marks = highlight(id, &text).expect("a grammar");
            for (line, marks) in text.iter().zip(&marks) {
                let mut at = 0;
                for (range, _) in marks {
                    assert!(range.start >= at && range.end <= line.len());
                    assert!(line.is_char_boundary(range.start) && line.is_char_boundary(range.end));
                    at = range.end;
                }
            }
        }
    }
}
