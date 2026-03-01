//! Integration tests for the `focusmute-cli` binary.
//!
//! These tests exercise the CLI binary via `assert_cmd`, verifying that
//! basic subcommands (help, version, config) produce expected output.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn cli() -> assert_cmd::Command {
    cargo_bin_cmd!("focusmute-cli")
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

// ── --verbose flag ──

#[test]
fn cli_verbose_flag_accepted() {
    cli().args(["-v", "config"]).assert().success();
}

#[test]
fn cli_verbose_long_flag_accepted() {
    cli().args(["--verbose", "config"]).assert().success();
}

// ── Subcommand integration tests ──
// Device-requiring commands tested via --help to avoid platform-dependent errors.

#[test]
fn cli_devices_succeeds() {
    cli().arg("devices").assert().success();
}

#[test]
fn cli_status_succeeds() {
    cli().arg("status").assert().success();
}

#[test]
fn cli_mute_help_succeeds() {
    cli()
        .args(["mute", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Mute"));
}

#[test]
fn cli_unmute_help_succeeds() {
    cli()
        .args(["unmute", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Unmute"));
}

#[test]
fn cli_descriptor_help_succeeds() {
    cli()
        .args(["descriptor", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("descriptor"));
}

#[test]
fn cli_probe_help_succeeds() {
    cli()
        .args(["probe", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Probe"));
}

#[test]
fn cli_monitor_help_succeeds() {
    cli()
        .args(["monitor", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("monitor"));
}

#[test]
fn cli_map_help_succeeds() {
    cli()
        .args(["map", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("map"));
}
