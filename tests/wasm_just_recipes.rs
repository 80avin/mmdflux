use std::process::Command;

#[test]
fn justfile_exposes_wasm_recipes() {
    let output = Command::new("just")
        .arg("--list")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run `just --list`");

    assert!(
        output.status.success(),
        "`just --list` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("wasm-build"),
        "`just --list` must include `wasm-build`"
    );
    assert!(
        stdout.contains("wasm-test"),
        "`just --list` must include `wasm-test`"
    );
}
