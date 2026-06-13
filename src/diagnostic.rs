#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub offset: usize,
    pub message: String,
}

impl Diagnostic {
    pub fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }

    pub fn location(&self, source: &str) -> (usize, usize) {
        let prefix = &source[..self.offset.min(source.len())];
        let line = prefix
            .chars()
            .filter(|character| *character == '\n')
            .count()
            + 1;
        let column = prefix
            .rsplit_once('\n')
            .map_or(prefix, |(_, line)| line)
            .chars()
            .count()
            + 1;
        (line, column)
    }
}

#[cfg(test)]
mod tests {
    use super::Diagnostic;

    #[test]
    fn calculates_unicode_line_and_column() {
        let source = "echo ok\necho café |";
        let offset = source.find('|').unwrap();
        assert_eq!(Diagnostic::new(offset, "error").location(source), (2, 11));
    }
}
