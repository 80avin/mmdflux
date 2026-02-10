use std::collections::HashMap;
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
    dependencies: Vec<CargoDependency>,
    features: HashMap<String, Vec<String>>,
    targets: Vec<CargoTarget>,
}

#[derive(Debug, Deserialize)]
struct CargoDependency {
    name: String,
    optional: bool,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
    #[serde(rename = "required-features", default)]
    required_features: Vec<String>,
}

fn mmdflux_metadata() -> CargoPackage {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run `cargo metadata`");

    assert!(
        output.status.success(),
        "`cargo metadata` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: CargoMetadata =
        serde_json::from_slice(&output.stdout).expect("failed to parse `cargo metadata` JSON");

    metadata
        .packages
        .into_iter()
        .find(|package| package.name == "mmdflux")
        .expect("mmdflux package missing from `cargo metadata` output")
}

#[test]
fn clap_dependency_is_optional() {
    let package = mmdflux_metadata();
    let clap = package
        .dependencies
        .iter()
        .find(|dependency| dependency.name == "clap")
        .expect("clap dependency not found");

    assert!(
        clap.optional,
        "clap must be optional so non-CLI consumers can disable it"
    );
}

#[test]
fn cli_feature_enables_clap_dependency() {
    let package = mmdflux_metadata();
    let cli_feature = package
        .features
        .get("cli")
        .expect("`cli` feature must be declared");
    let default_features = package
        .features
        .get("default")
        .expect("`default` feature set must be declared");

    assert!(
        cli_feature.iter().any(|feature| feature == "dep:clap"),
        "`cli` must include `dep:clap`"
    );
    assert!(
        default_features.iter().any(|feature| feature == "cli"),
        "`default` must include `cli` to preserve existing CLI behavior"
    );
}

#[test]
fn mmdflux_binary_requires_cli_feature() {
    let package = mmdflux_metadata();
    let mmdflux_bin = package
        .targets
        .iter()
        .find(|target| target.name == "mmdflux" && target.kind.iter().any(|kind| kind == "bin"))
        .expect("mmdflux binary target missing");

    assert!(
        mmdflux_bin
            .required_features
            .iter()
            .any(|feature| feature == "cli"),
        "mmdflux binary must require `cli` feature"
    );
}
