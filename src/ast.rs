#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub pipeline: Pipeline,
    pub connector: Option<Connector>,
    pub background: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connector {
    And,
    Or,
    Sequence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pipeline {
    pub commands: Vec<Command>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub words: Vec<Word>,
    pub redirects: Vec<Redirect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub parts: Vec<WordPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordPart {
    pub text: String,
    pub expansion: bool,
    pub split: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect {
    pub kind: RedirectKind,
    pub target: Word,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedirectKind {
    Stdin,
    Stdout,
    StdoutAppend,
    Stderr,
    StderrAppend,
    StderrToStdout,
}
