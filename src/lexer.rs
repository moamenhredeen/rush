use std::ops::Range;

use crate::ast::{Word, WordPart};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    Word(Word),
    Pipe,
    And,
    Or,
    Semi,
    Background,
    Input,
    Output,
    Append,
    ErrorOutput,
    ErrorAppend,
    ErrorToOutput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Range<usize>,
}

#[cfg(test)]
pub fn lex(source: &str) -> Result<Vec<Token>, String> {
    Ok(lex_spanned(source)?
        .into_iter()
        .map(|token| token.token)
        .collect())
}

pub fn lex_spanned(source: &str) -> Result<Vec<SpannedToken>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut byte_offsets: Vec<_> = source.char_indices().map(|(index, _)| index).collect();
    byte_offsets.push(source.len());
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index].is_whitespace() {
            if chars[index] == '\n'
                && !matches!(
                    tokens.last(),
                    Some(SpannedToken {
                        token: Token::Semi,
                        ..
                    })
                )
            {
                tokens.push(spanned(Token::Semi, index, index + 1, &byte_offsets));
            }
            index += 1;
            continue;
        }
        if chars[index] == '#' {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if let Some((token, consumed)) = operator(&chars[index..]) {
            tokens.push(spanned(token, index, index + consumed, &byte_offsets));
            index += consumed;
            continue;
        }

        let start = index;
        let (word, next) = word(&chars, index)?;
        tokens.push(spanned(Token::Word(word), start, next, &byte_offsets));
        index = next;
    }

    while matches!(
        tokens.last(),
        Some(SpannedToken {
            token: Token::Semi,
            ..
        })
    ) {
        tokens.pop();
    }
    Ok(tokens)
}

fn spanned(token: Token, start: usize, end: usize, offsets: &[usize]) -> SpannedToken {
    SpannedToken {
        token,
        span: offsets[start]..offsets[end],
    }
}

fn operator(chars: &[char]) -> Option<(Token, usize)> {
    let starts = |text: &str| chars.iter().take(text.len()).copied().eq(text.chars());
    for (text, token) in [
        ("2>&1", Token::ErrorToOutput),
        ("2>>", Token::ErrorAppend),
        ("2>", Token::ErrorOutput),
        ("&&", Token::And),
        ("||", Token::Or),
        (">>", Token::Append),
        ("|", Token::Pipe),
        (";", Token::Semi),
        ("&", Token::Background),
        ("<", Token::Input),
        (">", Token::Output),
    ] {
        if starts(text) {
            return Some((token, text.len()));
        }
    }
    None
}

fn word(chars: &[char], mut index: usize) -> Result<(Word, usize), String> {
    let mut parts = Vec::new();
    let mut plain = String::new();

    while index < chars.len() {
        if chars[index].is_whitespace() || operator(&chars[index..]).is_some() {
            break;
        }
        if chars[index] == '$' && chars.get(index + 1) == Some(&'(') {
            let end = substitution_end(chars, index + 2)?;
            plain.extend(chars[index..end].iter());
            index = end;
            continue;
        }
        match chars[index] {
            '\'' => {
                push_plain(&mut parts, &mut plain, false);
                index += 1;
                let start = index;
                while index < chars.len() && chars[index] != '\'' {
                    index += 1;
                }
                if index == chars.len() {
                    return Err("unterminated single quote".into());
                }
                parts.push(WordPart {
                    text: chars[start..index].iter().collect(),
                    expansion: false,
                    split: false,
                });
                index += 1;
            }
            '"' => {
                push_plain(&mut parts, &mut plain, false);
                index += 1;
                let mut quoted = String::new();
                while index < chars.len() && chars[index] != '"' {
                    if chars[index] == '$' && chars.get(index + 1) == Some(&'(') {
                        let end = substitution_end(chars, index + 2)?;
                        quoted.extend(chars[index..end].iter());
                        index = end;
                        continue;
                    }
                    if chars[index] == '\\' && index + 1 < chars.len() {
                        let escaped = chars[index + 1];
                        if matches!(escaped, '$' | '"' | '\\' | '\n') {
                            index += 1;
                        } else {
                            quoted.push('\\');
                            index += 1;
                            continue;
                        }
                    }
                    quoted.push(chars[index]);
                    index += 1;
                }
                if index == chars.len() {
                    return Err("unterminated double quote".into());
                }
                parts.push(WordPart {
                    text: quoted,
                    expansion: true,
                    split: false,
                });
                index += 1;
            }
            '\\' => {
                index += 1;
                if index == chars.len() {
                    return Err("trailing escape".into());
                }
                let escaped = chars[index];
                if !escaped.is_whitespace()
                    && operator(&chars[index..]).is_none()
                    && !matches!(escaped, '\\' | '\'' | '"' | '$' | '#')
                {
                    plain.push('\\');
                }
                plain.push(escaped);
                index += 1;
            }
            _ => {
                plain.push(chars[index]);
                index += 1;
            }
        }
    }
    push_plain(&mut parts, &mut plain, false);
    if parts.is_empty() {
        parts.push(WordPart {
            text: String::new(),
            expansion: false,
            split: false,
        });
    }
    Ok((Word { parts }, index))
}

fn substitution_end(chars: &[char], mut index: usize) -> Result<usize, String> {
    let mut depth = 1;
    let mut quote = None;
    while index < chars.len() {
        let current = chars[index];
        if let Some(active) = quote {
            if current == active {
                quote = None;
            } else if current == '\\' && active == '"' {
                index += 1;
            }
        } else {
            match current {
                '\'' | '"' => quote = Some(current),
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(index + 1);
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    Err("unterminated command substitution".into())
}

fn push_plain(parts: &mut Vec<WordPart>, plain: &mut String, allow_empty: bool) {
    if !plain.is_empty() || allow_empty {
        parts.push(WordPart {
            text: std::mem::take(plain),
            expansion: true,
            split: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_quotes_and_operators() {
        let tokens = lex("echo 'a b' \"$HOME\" | wc -c && echo ok").unwrap();
        assert!(matches!(tokens[3], Token::Pipe));
        assert!(matches!(tokens[6], Token::And));
        let Token::Word(word) = &tokens[1] else {
            panic!()
        };
        assert_eq!(word.parts[0].text, "a b");
        assert!(!word.parts[0].expansion);
    }

    #[test]
    fn keeps_command_substitution_together() {
        let tokens = lex("echo $(printf 'a b')").unwrap();
        let Token::Word(word) = &tokens[1] else {
            panic!()
        };
        assert_eq!(word.parts[0].text, "$(printf 'a b')");
    }

    #[test]
    fn preserves_non_special_double_quote_escapes() {
        let tokens = lex(r#"printf "a\nb""#).unwrap();
        let Token::Word(word) = &tokens[1] else {
            panic!()
        };
        assert_eq!(word.parts[0].text, r"a\nb");
    }

    #[test]
    fn preserves_windows_path_backslashes() {
        let tokens = lex(r"cd C:\Projects\rush").unwrap();
        let Token::Word(word) = &tokens[1] else {
            panic!()
        };
        assert_eq!(word.parts[0].text, r"C:\Projects\rush");
    }
}
