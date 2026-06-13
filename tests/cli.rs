use std::fs;
use std::process::Command;
#[cfg(unix)]
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

fn rush(source: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rush"))
        .args(["-c", source])
        .output()
        .unwrap()
}

#[test]
fn runs_bundled_pipeline() {
    let output = rush("echo hello | wc -w");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1");
}

#[test]
fn runs_logical_fallback() {
    let output = rush("rush-command-that-does-not-exist || echo recovered");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "recovered");
}

#[test]
fn runs_command_substitution() {
    let output = rush("echo \"words=$(echo hello | wc -w)\"");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "words=1");
}

#[test]
fn executes_script_and_redirects_output() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("rush-test-{unique}"));
    fs::create_dir(&directory).unwrap();
    let script = directory.join("script.rush");
    let output_file = directory.join("output.txt");
    fs::write(
        &script,
        "echo first > \"$RUSH_TEST_OUTPUT\"\necho second >> \"$RUSH_TEST_OUTPUT\"\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rush"))
        .arg(&script)
        .env("RUSH_TEST_OUTPUT", &output_file)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fs::read_to_string(&output_file).unwrap(), "first\nsecond\n");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn returns_missing_command_status() {
    let output = rush("rush-command-that-does-not-exist");
    assert_eq!(output.status.code(), Some(127));
}

#[test]
fn jobs_show_each_pipeline_label() {
    let output = rush("cat & echo ready & jobs");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\tcat\n"), "{stdout}");
    assert!(stdout.contains("\techo ready\n"), "{stdout}");
    assert!(!stdout.contains("cat & echo ready & jobs"), "{stdout}");
}

#[test]
fn fg_returns_completed_job_failure_status() {
    let output = rush("cat /rush-file-that-does-not-exist & fg %1");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "cat /rush-file-that-does-not-exist\n"
    );
}

#[test]
fn standalone_assignment_persists() {
    let output = rush("RUSH_ASSIGN=one; echo \"$RUSH_ASSIGN\"");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "one\n");
}

#[test]
fn quoted_assignment_value_is_not_split() {
    let output = rush("RUSH_ASSIGN=\"two words\"; echo \"$RUSH_ASSIGN\"");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "two words\n");
}

#[test]
fn command_assignment_is_temporary() {
    let rush_binary = env!("CARGO_BIN_EXE_rush");
    let source = format!(
        "RUSH_ASSIGN=outer; RUSH_ASSIGN=inner \"{rush_binary}\" -c 'echo \"$RUSH_ASSIGN\"'; echo \"$RUSH_ASSIGN\""
    );
    let output = rush(&source);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "inner\nouter\n");
}

#[test]
fn multiple_command_assignments_are_applied() {
    let rush_binary = env!("CARGO_BIN_EXE_rush");
    let source = format!(
        "RUSH_FIRST=one RUSH_SECOND=\"two words\" \"{rush_binary}\" -c 'echo \"$RUSH_FIRST/$RUSH_SECOND\"'"
    );
    let output = rush(&source);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "one/two words\n");
}

#[test]
fn assignment_applies_temporarily_to_stateful_builtin() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("rush-home-{unique}"));
    fs::create_dir(&directory).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rush"))
        .args(["-c", "HOME=\"$RUSH_TEST_HOME\" cd; pwd"])
        .env("RUSH_TEST_HOME", &directory)
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = fs::canonicalize(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    assert_eq!(actual, fs::canonicalize(&directory).unwrap());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn invalid_assignment_name_is_a_command() {
    let output = rush("1RUSH_ASSIGN=value");
    assert_eq!(output.status.code(), Some(127));
}

#[cfg(unix)]
#[test]
fn forwards_sigint_to_foreground_pipeline() {
    use std::os::unix::process::ExitStatusExt;

    let mut child = Command::new(env!("CARGO_BIN_EXE_rush"))
        .args(["-c", "sleep 10 | cat"])
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));

    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);

    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("rush did not interrupt its foreground pipeline");
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    assert_eq!(status.code(), Some(130), "signal={:?}", status.signal());
}

#[cfg(unix)]
#[test]
fn forwards_sigint_to_foregrounded_job() {
    use std::os::unix::process::ExitStatusExt;

    let mut child = Command::new(env!("CARGO_BIN_EXE_rush"))
        .args(["-c", "sleep 10 & fg %1"])
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));

    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);

    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("rush did not interrupt the foregrounded job");
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    assert_eq!(status.code(), Some(130), "signal={:?}", status.signal());
}
