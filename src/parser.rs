use crate::ast::*;
use crate::lexer::{SpannedToken, Token, lex_spanned};

pub fn parse(source: &str) -> Result<Program, String> {
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
    fn program(&mut self) -> Result<Program, String> {
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
                return Err("expected a command separator".into());
            }
        }
        Ok(Program { entries })
    }

    fn pipeline(&mut self) -> Result<Pipeline, String> {
        let mut commands = vec![self.command()?];
        while self.take(|t| matches!(t, Token::Pipe)) {
            commands.push(self.command()?);
        }
        Ok(Pipeline { commands })
    }

    fn command(&mut self) -> Result<Command, String> {
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
                            return Err("redirection requires a target".into());
                        };
                        self.index += 1;
                        redirects.push(Redirect { kind, target });
                    }
                }
                _ => break,
            }
        }
        if words.is_empty() {
            return Err("expected a command".into());
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
