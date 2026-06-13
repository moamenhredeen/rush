mod ast;
mod commands;
pub mod completion;
mod diagnostic;
mod expand;
pub mod highlighting;
mod lexer;
mod parser;
mod process_control;
mod shell;

pub use shell::Shell;
