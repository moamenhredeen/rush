use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::lexer::{SpannedToken, Token, lex_spanned};

pub fn parse(source: &str) -> Result<Program, Diagnostic> {
    let tokens = lex_spanned(source)?;
    let mut parser = Parser {
        source,
        tokens,
        index: 0,
    };
    parser.program()
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<SpannedToken>,
    index: usize,
}

impl Parser<'_> {
    fn program(&mut self) -> Result<Program, Diagnostic> {
        let mut entries = Vec::new();
        while self.index < self.tokens.len() {
            while self.take(|t| matches!(t, Token::Semi)) {}
            if self.index == self.tokens.len() {
                break;
            }
            let source_start = self.tokens[self.index].span.start;
            let pipeline = self.pipeline()?;
            let source_end = self
                .tokens
                .get(self.index)
                .map_or(self.source.len(), |token| token.span.start);
            let background = self.take(|t| matches!(t, Token::Background));
            let connector_token = self.tokens.get(self.index).cloned();
            let connector = if background || self.take(|t| matches!(t, Token::Semi)) {
                Some(Connector::Sequence)
            } else if self.take(|t| matches!(t, Token::And)) {
                Some(Connector::And)
            } else if self.take(|t| matches!(t, Token::Or)) {
                Some(Connector::Or)
            } else {
                None
            };
            entries.push(Entry {
                pipeline,
                source: self.source[source_start..source_end].trim().into(),
                connector,
                background,
            });
            if connector.is_none() && self.index < self.tokens.len() {
                let token = &self.tokens[self.index];
                return Err(Diagnostic::new(
                    token.span.start,
                    format!("unexpected operator `{}`", token_label(&token.token)),
                ));
            }
            if matches!(connector, Some(Connector::And | Connector::Or))
                && self.index == self.tokens.len()
            {
                let token = connector_token.expect("connector token exists");
                return Err(Diagnostic::new(
                    token.span.start,
                    format!("expected command after `{}`", token_label(&token.token)),
                ));
            }
        }
        Ok(Program { entries })
    }

    fn pipeline(&mut self) -> Result<Pipeline, Diagnostic> {
        let mut commands = vec![self.command()?];
        while let Some(pipe) = self.take_token(|t| matches!(t, Token::Pipe)) {
            match self.command() {
                Ok(command) => commands.push(command),
                Err(error)
                    if error.message == "expected a command"
                        || error.message.starts_with("unexpected operator") =>
                {
                    return Err(Diagnostic::new(
                        pipe.span.start,
                        "expected command after `|`",
                    ));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(Pipeline { commands })
    }

    fn command(&mut self) -> Result<Command, Diagnostic> {
        let mut words = Vec::new();
        let mut redirects = Vec::new();
        loop {
            match self
                .tokens
                .get(self.index)
                .cloned()
                .map(|token| token.token)
            {
                Some(Token::Word(word)) => {
                    words.push(word);
                    self.index += 1;
                }
                Some(token) if redirect_kind(&token).is_some() => {
                    let kind = redirect_kind(&token).unwrap();
                    let redirect = self.tokens[self.index].clone();
                    self.index += 1;
                    if kind == RedirectKind::StderrToStdout {
                        redirects.push(Redirect {
                            kind,
                            target: Word { parts: Vec::new() },
                        });
                    } else {
                        let Some(Token::Word(target)) = self
                            .tokens
                            .get(self.index)
                            .cloned()
                            .map(|token| token.token)
                        else {
                            return Err(Diagnostic::new(
                                redirect.span.start,
                                format!("expected path after `{}`", token_label(&redirect.token)),
                            ));
                        };
                        self.index += 1;
                        redirects.push(Redirect { kind, target });
                    }
                }
                _ => break,
            }
        }
        if words.is_empty() {
            if let Some(token) = self.tokens.get(self.index) {
                return Err(Diagnostic::new(
                    token.span.start,
                    format!("unexpected operator `{}`", token_label(&token.token)),
                ));
            }
            return Err(Diagnostic::new(self.source.len(), "expected a command"));
        }
        Ok(Command { words, redirects })
    }

    fn take(&mut self, predicate: impl FnOnce(&Token) -> bool) -> bool {
        if self
            .tokens
            .get(self.index)
            .is_some_and(|token| predicate(&token.token))
        {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn take_token(&mut self, predicate: impl FnOnce(&Token) -> bool) -> Option<SpannedToken> {
        let token = self.tokens.get(self.index)?;
        if !predicate(&token.token) {
            return None;
        }
        self.index += 1;
        Some(token.clone())
    }
}

fn redirect_kind(token: &Token) -> Option<RedirectKind> {
    Some(match token {
        Token::Input => RedirectKind::Stdin,
        Token::Output => RedirectKind::Stdout,
        Token::Append => RedirectKind::StdoutAppend,
        Token::ErrorOutput => RedirectKind::Stderr,
        Token::ErrorAppend => RedirectKind::StderrAppend,
        Token::ErrorToOutput => RedirectKind::StderrToStdout,
        _ => return None,
    })
}

fn token_label(token: &Token) -> &'static str {
    match token {
        Token::Word(_) => "word",
        Token::Pipe => "|",
        Token::And => "&&",
        Token::Or => "||",
        Token::Semi => ";",
        Token::Background => "&",
        Token::Input => "<",
        Token::Output => ">",
        Token::Append => ">>",
        Token::ErrorOutput => "2>",
        Token::ErrorAppend => "2>>",
        Token::ErrorToOutput => "2>&1",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pipeline_and_background() {
        let program = parse("echo hi | wc -c & echo done").unwrap();
        assert_eq!(program.entries.len(), 2);
        assert_eq!(program.entries[0].pipeline.commands.len(), 2);
        assert!(program.entries[0].background);
        assert_eq!(program.entries[0].source, "echo hi | wc -c");
        assert_eq!(program.entries[1].source, "echo done");
    }

    #[test]
    fn rejects_missing_redirect_target() {
        assert!(parse("echo hi >").is_err());
    }

    #[test]
    fn preserves_original_pipeline_source() {
        let program = parse("echo \"a & b\" | wc -c & jobs").unwrap();
        assert_eq!(program.entries[0].source, "echo \"a & b\" | wc -c");
        assert_eq!(program.entries[1].source, "jobs");
    }
}
