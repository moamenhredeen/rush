use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use reedline::{DefaultPrompt, DefaultPromptSegment, FileBackedHistory, Reedline, Signal};
use rush::Shell;

fn main() {
    let mut args = env::args_os().skip(1);
    let mut shell = Shell::new();

    let status = match args.next() {
        Some(arg) if arg == "-c" => match args.next() {
            Some(source) => shell.run_source(&source.to_string_lossy()),
            None => {
                eprintln!("rush: -c requires a command string");
                2
            }
        },
        Some(path) => run_script(&mut shell, PathBuf::from(path)),
        None if io::stdin().is_terminal() => run_repl(&mut shell),
        None => match io::read_to_string(io::stdin()) {
            Ok(source) => shell.run_source(&source),
            Err(error) => {
                eprintln!("rush: failed to read stdin: {error}");
                1
            }
        },
    };

    shell.shutdown_jobs();
    std::process::exit(status);
}

fn run_script(shell: &mut Shell, path: PathBuf) -> i32 {
    match fs::read_to_string(&path) {
        Ok(source) => shell.run_source(&source),
        Err(error) => {
            eprintln!("rush: {}: {error}", path.display());
            1
        }
    }
}

fn run_repl(shell: &mut Shell) -> i32 {
    let history = history_path()
        .and_then(|path| FileBackedHistory::with_file(1_000, path).ok())
        .map(Box::new);
    let mut editor = history
        .map(|history| Reedline::create().with_history(history))
        .unwrap_or_else(Reedline::create);
    let prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic("rush".into()),
        DefaultPromptSegment::Empty,
    );

    loop {
        shell.report_jobs();
        match editor.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                shell.run_source(&line);
                if let Some(status) = shell.take_exit_request() {
                    return status;
                }
            }
            Ok(Signal::CtrlC) => continue,
            Ok(Signal::CtrlD) => return shell.last_status(),
            Ok(_) => continue,
            Err(error) => {
                eprintln!("rush: failed to read input: {error}");
                return 1;
            }
        }
    }
}

fn history_path() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|path| path.join(".rush_history"))
}
