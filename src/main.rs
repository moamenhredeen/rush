use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use reedline::{
    ColumnarMenu, DefaultPrompt, DefaultPromptSegment, Emacs, FileBackedHistory, KeyCode,
    KeyModifiers, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal,
    default_emacs_keybindings,
};
use rush::{Shell, completion::RushCompleter};

fn main() {
    let mut args = env::args_os().skip(1);
    let mut shell = Shell::new();

    let status = match args.next() {
        Some(arg) if arg == "-c" => match args.next() {
            Some(source) => shell.run_source_named(&source.to_string_lossy(), "-c"),
            None => {
                eprintln!("rush: -c requires a command string");
                2
            }
        },
        Some(path) => run_script(&mut shell, PathBuf::from(path)),
        None if io::stdin().is_terminal() => run_repl(&mut shell),
        None => match io::read_to_string(io::stdin()) {
            Ok(source) => shell.run_source_named(&source, "<stdin>"),
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
        Ok(source) => shell.run_source_named(&source, &path.to_string_lossy()),
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
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".into()),
            ReedlineEvent::MenuNext,
        ]),
    );
    let completion_menu = ColumnarMenu::default().with_name("completion_menu");
    let editor = Reedline::create()
        .with_completer(Box::new(RushCompleter::new()))
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(completion_menu)))
        .with_edit_mode(Box::new(Emacs::new(keybindings)));
    let mut editor = match history {
        Some(history) => editor.with_history(history),
        None => editor,
    };
    let prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic("rush".into()),
        DefaultPromptSegment::Empty,
    );

    loop {
        shell.report_jobs();
        match editor.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                shell.run_source_named(&line, "<interactive>");
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
