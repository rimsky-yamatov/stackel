//! The `stackel` binary: flags and exit statuses.

use std::process::Command;

fn stackel(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_stackel"))
        .args(args)
        .output()
        .expect("the stackel binary runs")
}

#[test]
fn version_flag_prints_the_package_version() {
    for flag in ["--version", "-V"] {
        let out = stackel(&[flag]);
        assert!(out.status.success());
        assert_eq!(
            String::from_utf8(out.stdout).unwrap().trim(),
            format!("stackel {}", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn help_flag_prints_the_usage_and_succeeds() {
    for flag in ["--help", "-h"] {
        let out = stackel(&[flag]);
        assert!(out.status.success());
        assert!(String::from_utf8(out.stdout)
            .unwrap()
            .contains("usage: stackel run"));
    }
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let out = stackel(&["nonsense"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8(out.stderr)
        .unwrap()
        .contains("usage: stackel"));
}
