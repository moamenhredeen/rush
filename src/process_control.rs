use std::process::Command;
use std::sync::OnceLock;

#[cfg(unix)]
use std::sync::atomic::{AtomicI32, Ordering};

#[cfg(unix)]
static ACTIVE_GROUP: AtomicI32 = AtomicI32::new(0);

#[cfg(windows)]
static ACTIVE_GROUPS: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

static HANDLER_INSTALLED: OnceLock<bool> = OnceLock::new();

pub fn install_interrupt_handler() {
    let installed = *HANDLER_INSTALLED.get_or_init(|| ctrlc::set_handler(interrupt_active).is_ok());
    if !installed {
        eprintln!("rush: warning: failed to install Ctrl-C handler");
    }
}

pub struct ProcessGroup {
    #[cfg(unix)]
    id: i32,
    #[cfg(windows)]
    ids: Vec<u32>,
}

impl ProcessGroup {
    pub fn new() -> Self {
        Self {
            #[cfg(unix)]
            id: 0,
            #[cfg(windows)]
            ids: Vec::new(),
        }
    }

    pub fn configure(&self, command: &mut Command) {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(self.id);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            command.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }
    }

    pub fn add_child(&mut self, id: u32) {
        #[cfg(unix)]
        if self.id == 0 {
            self.id = id as i32;
        }
        #[cfg(windows)]
        self.ids.push(id);
    }

    pub fn activate(&self) -> ActiveProcessGroup {
        #[cfg(unix)]
        {
            ACTIVE_GROUP.store(self.id, Ordering::SeqCst);
            let terminal_group = terminal_foreground_group();
            if terminal_group.is_some() {
                set_terminal_foreground_group(self.id);
            }
            ActiveProcessGroup { terminal_group }
        }
        #[cfg(windows)]
        {
            *ACTIVE_GROUPS
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = self.ids.clone();
            ActiveProcessGroup {}
        }
    }
}

pub struct ActiveProcessGroup {
    #[cfg(unix)]
    terminal_group: Option<i32>,
}

impl Drop for ActiveProcessGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            if let Some(group) = self.terminal_group {
                set_terminal_foreground_group(group);
            }
            ACTIVE_GROUP.store(0, Ordering::SeqCst);
        }
        #[cfg(windows)]
        ACTIVE_GROUPS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }
}

#[cfg(unix)]
fn interrupt_active() {
    let group = ACTIVE_GROUP.load(Ordering::SeqCst);
    if group > 0 {
        unsafe {
            libc::kill(-group, libc::SIGINT);
        }
    }
}

#[cfg(windows)]
fn interrupt_active() {
    use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent};

    let groups = ACTIVE_GROUPS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for group in groups.iter().copied() {
        unsafe {
            GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, group);
        }
    }
}

#[cfg(unix)]
fn terminal_foreground_group() -> Option<i32> {
    let group = unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) };
    (group >= 0).then_some(group)
}

#[cfg(unix)]
fn set_terminal_foreground_group(group: i32) {
    unsafe {
        let mut blocked = std::mem::zeroed();
        let mut previous = std::mem::zeroed();
        libc::sigemptyset(&mut blocked);
        libc::sigaddset(&mut blocked, libc::SIGTTOU);
        libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, &mut previous);
        libc::tcsetpgrp(libc::STDIN_FILENO, group);
        libc::pthread_sigmask(libc::SIG_SETMASK, &previous, std::ptr::null_mut());
    }
}
