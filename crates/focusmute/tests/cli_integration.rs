//! Integration tests for the `focusmute-cli` binary.
//!
//! These tests exercise the CLI binary via `assert_cmd`, verifying that
//! basic subcommands (help, version, config) produce expected output.

use assert_cmd::Command;
use predicates::prelude::*;

fn cli() -> Command {
    Command::cargo_bin("focusmute-cli").expect("binary should be built")
}

#[test]
fn cli_help_succeeds() {
    cli()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("focusmute-cli"));
}

#[test]
fn cli_version_prints_version() {
    cli()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn cli_config_json_produces_valid_json() {
    let output = cli()
        .args(["--json", "config"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("config --json should produce valid JSON");
    assert!(
        json["settings"].is_object(),
        "JSON output should contain 'settings' object"
    );
    assert!(
        json["config_file"].is_string() || json["config_file"].is_null(),
        "config_file should be string or null"
    );
}
