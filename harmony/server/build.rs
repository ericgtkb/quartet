//! Compile score/*.proto into Rust with protox, a pure-Rust
//! protobuf compiler, so no system `protoc` is needed. Clients are used by tests.

use std::error::Error;
use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn Error>> {
    // The proto root is the repository root, so imports read "score/info.proto".
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let score = root.join("score");
    println!("cargo::rerun-if-changed={}", score.display());

    let mut files = Vec::new();
    for entry in std::fs::read_dir(&score)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "proto") {
            files.push(path.strip_prefix(&root)?.to_path_buf());
        }
    }
    files.sort();

    let fds = protox::compile(&files, [&root])?;
    tonic_prost_build::configure()
        .build_client(true)
        .compile_fds(fds)?;

    // Expose the compiler version for ServiceInfo.runtime.
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let output = Command::new(rustc).arg("--version").output()?;
    let version = String::from_utf8(output.stdout)?;
    let version = version.split_whitespace().nth(1).unwrap_or("unknown");
    println!("cargo::rustc-env=HARMONY_RUSTC_VERSION={version}");
    Ok(())
}
