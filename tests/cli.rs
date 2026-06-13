use std::fs;
use std::process::Command;
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
