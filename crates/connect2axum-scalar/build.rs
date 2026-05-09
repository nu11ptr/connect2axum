use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const SCALAR_VERSION: &str = "1.55.3";
const SCALAR_URL: &str = "https://cdn.jsdelivr.net/npm/@scalar/api-reference@1.55.3";

fn main() {
    println!("cargo:rerun-if-env-changed=CONNECT2AXUM_SCALAR_JS_PATH");
    println!("cargo:rerun-if-env-changed=DOCS_RS");
    println!("cargo:rustc-check-cfg=cfg(docsrs)");
    println!("cargo:rustc-env=CONNECT2AXUM_SCALAR_VERSION={SCALAR_VERSION}");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let out_file = out_dir.join("scalar-api-reference.js");

    if env::var_os("DOCS_RS").is_some() {
        println!("cargo:rustc-cfg=docsrs");
        return;
    }

    if let Some(source_path) = env::var_os("CONNECT2AXUM_SCALAR_JS_PATH") {
        fs::copy(&source_path, &out_file).unwrap_or_else(|err| {
            panic!(
                "failed to copy Scalar bundle from {}: {err}",
                PathBuf::from(source_path).display()
            )
        });
        return;
    }

    let output = Command::new("curl")
        .args(["-fsSL", SCALAR_URL])
        .output()
        .unwrap_or_else(|err| panic!("failed to start curl for {SCALAR_URL}: {err}"));

    if !output.status.success() {
        panic!(
            "failed to download Scalar bundle from {SCALAR_URL}: curl exited with {}",
            output.status
        );
    }

    fs::write(&out_file, output.stdout).unwrap_or_else(|err| {
        panic!(
            "failed to write Scalar bundle to {}: {err}",
            out_file.display()
        )
    });
}
