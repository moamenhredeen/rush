use std::env;

use crate::ast::Word;

pub fn expand_word(
    word: &Word,
    substitute: &mut impl FnMut(&str) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    let mut value = String::new();
    let mut split = false;
    for part in &word.parts {
        let expanded = if part.expansion {
            expand_text(&part.text, substitute)?
        } else {
            part.text.clone()
        };
        value.push_str(&expanded);
        split |= part.split;
    }
    if split {
        let fields: Vec<_> = value.split_whitespace().map(str::to_owned).collect();
        if !fields.is_empty() {
            return Ok(fields);
        }
    }
    Ok(vec![value])
}

fn expand_text(
    text: &str,
    substitute: &mut impl FnMut(&str) -> Result<String, String>,
) -> Result<String, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '$' {
            output.push(chars[index]);
            index += 1;
            continue;
        }
        if chars.get(index + 1) == Some(&'(') {
            let (source, next) = command_substitution(&chars, index + 2)?;
            output.push_str(substitute(&source)?.trim_end_matches(['\r', '\n']));
            index = next;
        } else if chars.get(index + 1) == Some(&'{') {
            let start = index + 2;
            let Some(offset) = chars[start..].iter().position(|c| *c == '}') else {
                return Err("unterminated variable expansion".into());
            };
            let end = start + offset;
            output.push_str(
                &env::var(chars[start..end].iter().collect::<String>()).unwrap_or_default(),
            );
            index = end + 1;
        } else {
            let start = index + 1;
            let mut end = start;
            while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            if end == start {
                output.push('$');
                index += 1;
            } else {
                output.push_str(
                    &env::var(chars[start..end].iter().collect::<String>()).unwrap_or_default(),
                );
                index = end;
            }
        }
    }
    Ok(output)
}

fn command_substitution(chars: &[char], mut index: usize) -> Result<(String, usize), String> {
    let start = index;
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
                        return Ok((chars[start..index].iter().collect(), index + 1));
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    Err("unterminated command substitution".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::WordPart;

    #[test]
    fn expands_variables_and_substitution() {
        unsafe { env::set_var("RUSH_TEST_VALUE", "hello") };
        let word = Word {
            parts: vec![WordPart {
                text: "$RUSH_TEST_VALUE-$(echo x)".into(),
                expansion: true,
                split: false,
            }],
        };
        let expanded = expand_word(&word, &mut |_| Ok("world\n".into())).unwrap();
        assert_eq!(expanded, ["hello-world"]);
    }
}
