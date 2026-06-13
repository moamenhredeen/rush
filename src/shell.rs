use std::collections::BTreeMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, Stdio};

use glob::glob;
use os_pipe::{PipeReader, PipeWriter, pipe};

use crate::ast::*;
use crate::expand::expand_word;
use crate::parser::parse;

pub struct Shell {
    last_status: i32,
    exit_request: Option<i32>,
    jobs: BTreeMap<u32, Job>,
    next_job: u32,
}

struct Job {
    command: String,
    children: Vec<Child>,
    status: Option<i32>,
    announced: bool,
}

struct ExpandedCommand {
    argv: Vec<String>,
    assignments: Vec<(String, String)>,
    redirects: Vec<ExpandedRedirect>,
}

struct ExpandedRedirect {
    kind: RedirectKind,
    target: String,
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell {
    pub fn new() -> Self {
        Self {
            last_status: 0,
            exit_request: None,
            jobs: BTreeMap::new(),
            next_job: 1,
        }
    }

    pub fn last_status(&self) -> i32 {
        self.last_status
    }
    pub fn take_exit_request(&mut self) -> Option<i32> {
        self.exit_request.take()
    }

    pub fn run_source(&mut self, source: &str) -> i32 {
        let program = match parse(source) {
            Ok(program) => program,
            Err(error) => {
                eprintln!("rush: syntax error: {error}");
                self.last_status = 2;
                return 2;
            }
        };

        let mut previous_connector = None;
        for entry in program.entries {
            let should_run = match previous_connector {
                Some(Connector::And) => self.last_status == 0,
                Some(Connector::Or) => self.last_status != 0,
                _ => true,
            };
            if should_run {
                self.last_status =
                    self.execute_pipeline(entry.pipeline, entry.background, source.trim());
                if self.exit_request.is_some() {
                    break;
                }
            }
            previous_connector = entry.connector;
        }
        self.last_status
    }

    fn execute_pipeline(&mut self, pipeline: Pipeline, background: bool, source: &str) -> i32 {
        let expanded = match pipeline
            .commands
            .iter()
            .map(|command| self.expand_command(command))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(commands) => commands,
            Err(error) => {
                eprintln!("rush: {error}");
                return 1;
            }
        };
        if expanded.len() == 1 && !background {
            if let Some(status) = self.run_builtin(&expanded[0]) {
                return status;
            }
            if expanded[0].argv.is_empty() {
                for (name, value) in &expanded[0].assignments {
                    unsafe { env::set_var(name, value) };
                }
                return 0;
            }
        }
        if expanded.iter().any(|command| {
            command
                .argv
                .first()
                .is_some_and(|name| is_stateful_builtin(name))
        }) {
            eprintln!("rush: stateful built-ins cannot run in pipelines or background jobs");
            return 2;
        }
        match spawn_pipeline(expanded, background) {
            Ok(children) if background => {
                let id = self.next_job;
                self.next_job += 1;
                let pids: Vec<_> = children.iter().map(Child::id).collect();
                eprintln!(
                    "[{id}] {}",
                    pids.iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                self.jobs.insert(
                    id,
                    Job {
                        command: source.into(),
                        children,
                        status: None,
                        announced: false,
                    },
                );
                0
            }
            Ok(mut children) => wait_pipeline(&mut children),
            Err(error) => {
                eprintln!("rush: {error}");
                command_error_status(&error)
            }
        }
    }

    fn expand_command(&mut self, command: &crate::ast::Command) -> Result<ExpandedCommand, String> {
        let mut argv = Vec::new();
        for word in &command.words {
            let fields = expand_word(word, &mut |source| self.capture(source))?;
            argv.extend(expand_globs(word, fields));
        }
        let mut assignments = Vec::new();
        while argv.first().is_some_and(|word| assignment(word).is_some()) {
            let item = argv.remove(0);
            let (name, value) = assignment(&item).unwrap();
            assignments.push((name.into(), value.into()));
        }
        let redirects = command
            .redirects
            .iter()
            .map(|redirect| {
                let target = if redirect.kind == RedirectKind::StderrToStdout {
                    String::new()
                } else {
                    let fields = expand_word(&redirect.target, &mut |source| self.capture(source))?;
                    if fields.len() != 1 {
                        return Err("ambiguous redirect".into());
                    }
                    fields[0].clone()
                };
                Ok(ExpandedRedirect {
                    kind: redirect.kind,
                    target,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(ExpandedCommand {
            argv,
            assignments,
            redirects,
        })
    }

    fn capture(&mut self, source: &str) -> Result<String, String> {
        let output = ProcessCommand::new(env::current_exe().map_err(|e| e.to_string())?)
            .arg("-c")
            .arg(source)
            .output()
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn run_builtin(&mut self, command: &ExpandedCommand) -> Option<i32> {
        let name = command.argv.first()?;
        match name.as_str() {
            "cd" => Some(builtin_cd(&command.argv)),
            "exit" => {
                let status = command.argv.get(1).map(|value| value.parse()).transpose();
                match status {
                    Ok(status) => {
                        let status = status.unwrap_or(self.last_status);
                        self.exit_request = Some(status);
                        Some(status)
                    }
                    Err(_) => {
                        eprintln!("rush: exit: numeric argument required");
                        Some(2)
                    }
                }
            }
            "jobs" => {
                self.print_jobs();
                Some(0)
            }
            "fg" => Some(self.foreground(command.argv.get(1).map(String::as_str))),
            _ => None,
        }
    }

    pub fn report_jobs(&mut self) {
        self.poll_jobs();
        for (id, job) in &mut self.jobs {
            if job.status.is_some() && !job.announced {
                eprintln!("[{id}] Done\t{}", job.command);
                job.announced = true;
            }
        }
        self.jobs
            .retain(|_, job| !(job.status.is_some() && job.announced));
    }

    fn print_jobs(&mut self) {
        self.poll_jobs();
        for (id, job) in &self.jobs {
            let state = if job.status.is_some() {
                "Done"
            } else {
                "Running"
            };
            println!("[{id}] {state}\t{}", job.command);
        }
    }

    fn poll_jobs(&mut self) {
        for job in self.jobs.values_mut() {
            if job.status.is_some() {
                continue;
            }
            let mut complete = true;
            let mut status = 0;
            for child in &mut job.children {
                match child.try_wait() {
                    Ok(Some(exit)) => status = exit.code().unwrap_or(1),
                    Ok(None) => complete = false,
                    Err(_) => {
                        status = 1;
                    }
                }
            }
            if complete {
                job.status = Some(status);
            }
        }
    }

    fn foreground(&mut self, spec: Option<&str>) -> i32 {
        self.poll_jobs();
        let id = match spec {
            Some(value) => value.strip_prefix('%').unwrap_or(value).parse().ok(),
            None => self.jobs.keys().next_back().copied(),
        };
        let Some(id) = id else {
            eprintln!("rush: fg: no current job");
            return 1;
        };
        let Some(mut job) = self.jobs.remove(&id) else {
            eprintln!("rush: fg: %{id}: no such job");
            return 1;
        };
        println!("{}", job.command);
        job.status
            .unwrap_or_else(|| wait_pipeline(&mut job.children))
    }

    pub fn shutdown_jobs(&mut self) {
        for job in self.jobs.values_mut() {
            for child in &mut job.children {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        self.jobs.clear();
    }
}

fn spawn_pipeline(commands: Vec<ExpandedCommand>, background: bool) -> io::Result<Vec<Child>> {
    let count = commands.len();
    let mut children = Vec::new();
    let mut previous_stdout: Option<PipeReader> = None;
    for (index, command) in commands.into_iter().enumerate() {
        if command.argv.is_empty() {
            return Err(io::Error::other("empty command in pipeline"));
        }
        let (program, bundled) = command_program(&command.argv[0])?;
        let mut process = ProcessCommand::new(program);
        if bundled {
            process.arg(&command.argv[0]);
        }
        process.args(&command.argv[1..]);
        process.envs(command.assignments);
        let mut stdin = previous_stdout.take().map(Stdio::from);
        if stdin.is_none() && background {
            stdin = Some(Stdio::null());
        }
        let (next_reader, next_writer) = if index + 1 < count {
            let (reader, writer) = pipe()?;
            (Some(reader), Some(writer))
        } else {
            (None, None)
        };
        let mut stdout = next_writer.map_or(OutputTarget::Inherit, OutputTarget::Pipe);
        let mut stderr = OutputTarget::Inherit;
        configure_redirects(&command.redirects, &mut stdin, &mut stdout, &mut stderr)?;
        if let Some(stdin) = stdin {
            process.stdin(stdin);
        }
        process.stdout(stdout.stdio()?);
        process.stderr(stderr.stdio()?);
        children.push(process.spawn()?);
        previous_stdout = next_reader;
    }
    Ok(children)
}

enum OutputTarget {
    Inherit,
    File(File),
    Pipe(PipeWriter),
}

impl OutputTarget {
    fn stdio(&self) -> io::Result<Stdio> {
        match self {
            Self::Inherit => Ok(Stdio::inherit()),
            Self::File(file) => Ok(Stdio::from(file.try_clone()?)),
            Self::Pipe(writer) => Ok(Stdio::from(writer.try_clone()?)),
        }
    }

    fn duplicate(&self) -> io::Result<Self> {
        match self {
            Self::Inherit => Ok(Self::Inherit),
            Self::File(file) => Ok(Self::File(file.try_clone()?)),
            Self::Pipe(writer) => Ok(Self::Pipe(writer.try_clone()?)),
        }
    }
}

fn configure_redirects(
    redirects: &[ExpandedRedirect],
    stdin: &mut Option<Stdio>,
    stdout: &mut OutputTarget,
    stderr: &mut OutputTarget,
) -> io::Result<()> {
    for redirect in redirects {
        match redirect.kind {
            RedirectKind::Stdin => {
                *stdin = Some(Stdio::from(File::open(&redirect.target)?));
            }
            RedirectKind::Stdout => {
                *stdout = OutputTarget::File(File::create(&redirect.target)?);
            }
            RedirectKind::StdoutAppend => {
                *stdout = OutputTarget::File(append(&redirect.target)?);
            }
            RedirectKind::Stderr => {
                *stderr = OutputTarget::File(File::create(&redirect.target)?);
            }
            RedirectKind::StderrAppend => {
                *stderr = OutputTarget::File(append(&redirect.target)?);
            }
            RedirectKind::StderrToStdout => {
                *stderr = stdout.duplicate()?;
            }
        }
    }
    Ok(())
}

fn append(path: impl AsRef<Path>) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn wait_pipeline(children: &mut [Child]) -> i32 {
    let last = children.len().saturating_sub(1);
    let mut status = 1;
    for (index, child) in children.iter_mut().enumerate() {
        match child.wait() {
            Ok(exit) if index == last => status = exit.code().unwrap_or(1),
            Err(error) => {
                eprintln!("rush: failed waiting for process: {error}");
                status = 1;
            }
            _ => {}
        }
    }
    status
}

fn builtin_cd(argv: &[String]) -> i32 {
    if argv.len() > 2 {
        eprintln!("rush: cd: too many arguments");
        return 2;
    }
    let target = argv
        .get(1)
        .cloned()
        .or_else(|| env::var("HOME").ok())
        .or_else(|| env::var("USERPROFILE").ok());
    let Some(target) = target else {
        eprintln!("rush: cd: home directory is not set");
        return 1;
    };
    match env::set_current_dir(&target) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("rush: cd: {target}: {error}");
            1
        }
    }
}

fn assignment(word: &str) -> Option<(&str, &str)> {
    let (name, value) = word.split_once('=')?;
    (!name.is_empty()
        && name
            .chars()
            .enumerate()
            .all(|(i, c)| c == '_' || c.is_ascii_alphanumeric() && (i > 0 || !c.is_ascii_digit())))
    .then_some((name, value))
}

fn expand_globs(word: &Word, fields: Vec<String>) -> Vec<String> {
    let can_glob = word.parts.iter().any(|part| part.split);
    if !can_glob {
        return fields;
    }
    fields
        .into_iter()
        .flat_map(|field| {
            if !field.contains(['*', '?', '[']) {
                return vec![field];
            }
            let matches: Vec<_> = glob(&field)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            if matches.is_empty() {
                vec![field]
            } else {
                matches
            }
        })
        .collect()
}

fn is_stateful_builtin(name: &str) -> bool {
    matches!(name, "cd" | "exit" | "jobs" | "fg")
}

fn command_program(name: &str) -> io::Result<(std::path::PathBuf, bool)> {
    const BUNDLED: &[&str] = &[
        "cat", "cp", "echo", "ls", "mkdir", "mv", "pwd", "rm", "sort", "touch", "uniq", "wc",
    ];
    let is_explicit = Path::new(name).is_absolute()
        || name.contains(std::path::MAIN_SEPARATOR)
        || (cfg!(windows) && name.contains('/'));
    if is_explicit || !BUNDLED.contains(&name) {
        return Ok((name.into(), false));
    }
    let mut companion = env::current_exe()?;
    companion.set_file_name(if cfg!(windows) {
        "rush-utils.exe"
    } else {
        "rush-utils"
    });
    if !companion.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "bundled utility companion is missing: {}",
                companion.display()
            ),
        ));
    }
    Ok((companion, true))
}

fn command_error_status(error: &io::Error) -> i32 {
    match error.kind() {
        io::ErrorKind::NotFound => 127,
        io::ErrorKind::PermissionDenied => 126,
        _ => 1,
    }
}
