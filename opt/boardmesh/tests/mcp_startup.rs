use std::fs;
use std::process::{Command, Stdio};

fn startup(args: &[&str], check: impl FnOnce(std::process::Output)) {
    let directory =
        std::env::temp_dir().join(format!("boardmesh-startup-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_boardmesh-mcp"))
        .args(args)
        .current_dir(&directory)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let created: Vec<_> = fs::read_dir(&directory).unwrap().collect();
    fs::remove_dir_all(&directory).unwrap();
    check(output);
    assert!(
        created.is_empty(),
        "startup flags must not create board data"
    );
}

#[test]
fn help_exits_successfully_without_starting_a_server() {
    startup(&["--help"], |output| {
        assert!(output.status.success(), "{:?}", output);
        assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
    });
}

#[test]
fn unknown_flag_is_rejected_before_creating_board_data() {
    startup(&["--not-a-boardmesh-option", "--no-runtime-ws"], |output| {
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unknown argument"));
    });
}

#[test]
fn missing_option_values_are_rejected_before_creating_board_data() {
    for flag in ["--session", "--scene", "--sessions-dir", "--runtime-addr"] {
        for arguments in [vec![flag], vec![flag, "--no-runtime-ws"], vec![flag, "-h"]] {
            startup(&arguments, |output| {
                assert!(!output.status.success());
                assert!(String::from_utf8_lossy(&output.stderr).contains("requires a value"));
            });
        }
    }
}
