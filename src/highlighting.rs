use nu_ansi_term::{Color, Style};
use reedline::{Highlighter, StyledText};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HighlightKind {
    Plain,
    Command,
    Assignment,
    Operator,
    String,
    Variable,
    Comment,
    Error,
}

#[derive(Default)]
pub struct RushHighlighter;

impl Highlighter for RushHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled = StyledText::new();
        for (kind, text) in segments(line) {
            styled.push((style(kind), text.into()));
        }
        styled
    }

    fn is_inside_string_literal(&self, line: &str, cursor: usize) -> bool {
        let cursor = floor_char_boundary(line, cursor.min(line.len()));
        segments(&line[..cursor])
            .last()
            .is_some_and(|(kind, text)| {
                *kind == HighlightKind::Error && (text.starts_with('\'') || text.starts_with('"'))
            })
    }
}

fn style(kind: HighlightKind) -> Style {
    match kind {
        HighlightKind::Plain => Style::default(),
        HighlightKind::Command => Style::new().fg(Color::Green).bold(),
        HighlightKind::Assignment => Style::new().fg(Color::Blue),
        HighlightKind::Operator => Style::new().fg(Color::Purple).bold(),
        HighlightKind::String => Style::new().fg(Color::Yellow),
        HighlightKind::Variable => Style::new().fg(Color::Cyan),
        HighlightKind::Comment => Style::new().fg(Color::DarkGray).italic(),
        HighlightKind::Error => Style::new().fg(Color::Red).underline(),
    }
}

fn segments(line: &str) -> Vec<(HighlightKind, &str)> {
    let mut segments = Vec::new();
    let mut index = 0;
    let mut command_position = true;

    while index < line.len() {
        let character = line[index..]
            .chars()
            .next()
            .expect("valid character boundary");
        if character.is_whitespace() {
            let end = take_while(line, index, char::is_whitespace);
            segments.push((HighlightKind::Plain, &line[index..end]));
            index = end;
            continue;
        }
        if character == '#' {
            segments.push((HighlightKind::Comment, &line[index..]));
            break;
        }
        if let Some(operator) = operator_at(&line[index..]) {
            let end = index + operator.len();
            segments.push((HighlightKind::Operator, &line[index..end]));
            if matches!(operator, "|" | "||" | "&&" | ";" | "&") {
                command_position = true;
            }
            index = end;
            continue;
        }
        if matches!(character, '\'' | '"') {
            let (end, closed) = quoted_end(line, index, character);
            let kind = if closed {
                HighlightKind::String
            } else {
                HighlightKind::Error
            };
            segments.push((kind, &line[index..end]));
            command_position = false;
            index = end;
            continue;
        }
        if character == '$' {
            let (end, closed) = variable_end(line, index);
            segments.push((
                if closed {
                    HighlightKind::Variable
                } else {
                    HighlightKind::Error
                },
                &line[index..end],
            ));
            index = end;
            continue;
        }

        let end = word_end(line, index);
        let word = &line[index..end];
        let kind = if command_position && assignment_word(word) {
            HighlightKind::Assignment
        } else if command_position {
            command_position = false;
            HighlightKind::Command
        } else {
            HighlightKind::Plain
        };
        segments.push((kind, word));
        index = end;
    }
    segments
}

fn operator_at(text: &str) -> Option<&'static str> {
    [
        "2>&1", "2>>", "&&", "||", ">>", "2>", "|", ";", "&", "<", ">",
    ]
    .into_iter()
    .find(|operator| text.starts_with(operator))
}

fn word_end(line: &str, mut index: usize) -> usize {
    let mut escaped = false;
    while index < line.len() {
        let character = line[index..]
            .chars()
            .next()
            .expect("valid character boundary");
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character.is_whitespace()
            || matches!(character, '\'' | '"' | '$' | '|' | '&' | ';' | '<' | '>')
        {
            break;
        }
        index += character.len_utf8();
    }
    index
}

fn quoted_end(line: &str, start: usize, quote: char) -> (usize, bool) {
    let mut index = start + quote.len_utf8();
    let mut escaped = false;
    while index < line.len() {
        let character = line[index..]
            .chars()
            .next()
            .expect("valid character boundary");
        index += character.len_utf8();
        if escaped {
            escaped = false;
        } else if character == '\\' && quote == '"' {
            escaped = true;
        } else if character == quote {
            return (index, true);
        }
    }
    (line.len(), false)
}

fn variable_end(line: &str, start: usize) -> (usize, bool) {
    let next = start + 1;
    if line[next..].starts_with('?') {
        return (next + 1, true);
    }
    if line[next..].starts_with('{') {
        return line[next + 1..]
            .find('}')
            .map_or((line.len(), false), |offset| (next + offset + 2, true));
    }
    if line[next..].starts_with('(') {
        let mut depth = 1;
        for (offset, character) in line[next + 1..].char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return (next + 1 + offset + 1, true);
                    }
                }
                _ => {}
            }
        }
        return (line.len(), false);
    }
    let end = take_while(line, next, |character| {
        character == '_' || character.is_ascii_alphanumeric()
    });
    if end == next {
        (next, true)
    } else {
        (end, true)
    }
}

fn assignment_word(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name.chars().enumerate().all(|(index, character)| {
            character == '_'
                || character.is_ascii_alphabetic()
                || index > 0 && character.is_ascii_digit()
        })
}

fn take_while(line: &str, mut index: usize, predicate: impl Fn(char) -> bool) -> usize {
    while index < line.len() {
        let character = line[index..]
            .chars()
            .next()
            .expect("valid character boundary");
        if !predicate(character) {
            break;
        }
        index += character.len_utf8();
    }
    index
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_shell_syntax_and_preserves_text() {
        let line = "NAME=value echo \"$HOME\" | wc -c # count";
        let highlighted = segments(line);
        assert_eq!(
            highlighted,
            [
                (HighlightKind::Assignment, "NAME=value"),
                (HighlightKind::Plain, " "),
                (HighlightKind::Command, "echo"),
                (HighlightKind::Plain, " "),
                (HighlightKind::String, "\"$HOME\""),
                (HighlightKind::Plain, " "),
                (HighlightKind::Operator, "|"),
                (HighlightKind::Plain, " "),
                (HighlightKind::Command, "wc"),
                (HighlightKind::Plain, " "),
                (HighlightKind::Plain, "-c"),
                (HighlightKind::Plain, " "),
                (HighlightKind::Comment, "# count"),
            ]
        );
        assert_eq!(
            highlighted
                .iter()
                .map(|(_, text)| *text)
                .collect::<String>(),
            line
        );
    }

    #[test]
    fn marks_incomplete_syntax_as_error() {
        assert_eq!(
            segments("echo \"unfinished").last(),
            Some(&(HighlightKind::Error, "\"unfinished"))
        );
        assert_eq!(
            segments("echo $(unfinished").last(),
            Some(&(HighlightKind::Error, "$(unfinished"))
        );
    }

    #[test]
    fn highlights_variables_and_token_boundary_comments() {
        assert_eq!(
            segments("echo pre$HOME ${USER} $? # comment"),
            [
                (HighlightKind::Command, "echo"),
                (HighlightKind::Plain, " "),
                (HighlightKind::Plain, "pre"),
                (HighlightKind::Variable, "$HOME"),
                (HighlightKind::Plain, " "),
                (HighlightKind::Variable, "${USER}"),
                (HighlightKind::Plain, " "),
                (HighlightKind::Variable, "$?"),
                (HighlightKind::Plain, " "),
                (HighlightKind::Comment, "# comment"),
            ]
        );
        assert_eq!(
            segments("echo name#part").last(),
            Some(&(HighlightKind::Plain, "name#part"))
        );
    }

    #[test]
    fn detects_cursor_inside_strings() {
        let highlighter = RushHighlighter;
        assert!(highlighter.is_inside_string_literal("echo \"hello", 11));
        assert!(!highlighter.is_inside_string_literal("echo \"hello\"", 12));
    }
}
