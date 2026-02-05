use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn mmdflux() -> Command {
    cargo_bin_cmd!("mmdflux")
}

#[test]
fn test_lint_valid_input_exit_0() {
    mmdflux()
        .arg("--lint")
        .write_stdin("graph TD\nA --> B\n")
        .assert()
        .success();
}

#[test]
fn test_lint_invalid_input_exit_1() {
    mmdflux()
        .arg("--lint")
        .write_stdin("not valid mermaid")
        .assert()
        .failure();
}

#[test]
fn test_lint_human_readable_error_on_stderr() {
    mmdflux()
        .arg("--lint")
        .write_stdin("not valid mermaid")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn test_lint_valid_no_output() {
    mmdflux()
        .arg("--lint")
        .write_stdin("graph TD\nA --> B\n")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn test_lint_with_warnings() {
    mmdflux()
        .arg("--lint")
        .write_stdin("graph TD\nA --> B\nstyle A fill:#f9f\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("warning"));
}
