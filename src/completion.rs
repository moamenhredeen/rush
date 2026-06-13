use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use reedline::{Completer, Span, Suggestion};

use crate::commands::{BUILTINS, BUNDLED};

pub struct RushCompleter {
    commands: Vec<String>,
}

impl Default for RushCompleter {
    fn default() -> Self {
        Self::new()
    }
}

impl RushCompleter {
    pub fn new() -> Self {
        Self {
            commands: command_names(),
        }
    }

    #[cfg(test)]
    fn with_commands(commands: &[&str]) -> Self {
        let mut commands: Vec<_> = commands.iter().map(|command| (*command).into()).collect();
        commands.sort();
        commands.dedup();
        Self { commands }
    }
}

impl Completer for RushCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let pos = floor_char_boundary(line, pos.min(line.len()));
        let start = token_start(line, pos);
        let fragment = &line[start..pos];
        let values = if command_position(&line[..start]) && !fragment.contains(['/', '\\']) {
            self.commands
                .iter()
                .filter(|command| command.starts_with(fragment))
                .cloned()
                .collect()
        } else {
            path_completions(
                fragment,
                &env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            )
        };
        values
            .into_iter()
            .map(|value| Suggestion {
                value,
                span: Span::new(start, pos),
                append_whitespace: true,
                ..Suggestion::default()
            })
            .collect()
    }
}

fn command_names() -> Vec<String> {
    let mut names: BTreeSet<String> = BUILTINS
        .iter()
        .chain(BUNDLED)
        .map(|name| (*name).into())
        .collect();
    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path) {
            let Ok(entries) = fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                if is_executable(&entry.path())
                    && let Some(name) = command_name(&entry.path())
                {
                    names.insert(name);
                }
            }
        }
    }
    names.into_iter().collect()
}

fn command_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    #[cfg(windows)]
    {
        let path = Path::new(name.as_ref());
        let extension = path.extension()?.to_string_lossy();
        if !["exe", "com", "cmd", "bat"]
            .iter()
            .any(|value| extension.eq_ignore_ascii_case(value))
        {
            return None;
        }
        return Some(path.file_stem()?.to_string_lossy().into_owned());
    }
    #[cfg(not(windows))]
    Some(name.into_owned())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    path.is_file() && command_name(path).is_some()
}

fn token_start(line: &str, pos: usize) -> usize {
    let mut start = 0;
    let mut escaped = false;
    for (index, character) in line[..pos].char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character.is_whitespace() || matches!(character, '|' | '&' | ';' | '<' | '>') {
            start = index + character.len_utf8();
        }
    }
    start
}

fn command_position(prefix: &str) -> bool {
    let trimmed = prefix.trim_end();
    trimmed.is_empty()
        || trimmed.ends_with('|')
        || trimmed.ends_with('&')
        || trimmed.ends_with(';')
        || trimmed.split_whitespace().all(valid_assignment_prefix)
}

fn valid_assignment_prefix(word: &str) -> bool {
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

fn path_completions(fragment: &str, cwd: &Path) -> Vec<String> {
    let fragment_path = Path::new(fragment);
    let (typed_directory, search_directory, prefix) = match fragment_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => (
            parent.to_path_buf(),
            if parent.is_absolute() {
                parent.to_path_buf()
            } else {
                cwd.join(parent)
            },
            fragment_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
        ),
        _ => (PathBuf::new(), cwd.to_path_buf(), fragment.into()),
    };
    let Ok(entries) = fs::read_dir(search_directory) else {
        return Vec::new();
    };
    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(prefix.as_ref()) {
            continue;
        }
        let mut value = typed_directory.join(name).to_string_lossy().into_owned();
        value = value.replace(' ', "\\ ");
        if entry.path().is_dir() {
            value.push(std::path::MAIN_SEPARATOR);
        }
        matches.push(value);
    }
    matches.sort();
    matches
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

    fn values(completer: &mut RushCompleter, line: &str) -> Vec<String> {
        completer
            .complete(line, line.len())
            .into_iter()
            .map(|suggestion| suggestion.value)
            .collect()
    }

    #[test]
    fn completes_commands_at_command_positions() {
        let mut completer = RushCompleter::with_commands(&["echo", "exit", "env"]);
        assert_eq!(values(&mut completer, "e"), ["echo", "env", "exit"]);
        assert_eq!(values(&mut completer, "echo ok | ex"), ["exit"]);
        assert_eq!(values(&mut completer, "NAME=value ec"), ["echo"]);
    }

    #[test]
    fn completes_paths_after_commands() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("alpha file"), "").unwrap();
        fs::create_dir(directory.path().join("alpha-dir")).unwrap();
        let matches = path_completions("alpha", directory.path());
        assert_eq!(
            matches,
            [
                format!("alpha-dir{}", std::path::MAIN_SEPARATOR),
                "alpha\\ file".into()
            ]
        );
    }
}
