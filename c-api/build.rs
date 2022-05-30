use std::env;
use std::path::{PathBuf};

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    if target_os == "windows" {
        let mut sspi_def_file = manifest_dir.clone();
        sspi_def_file.push("src");
        sspi_def_file.push("sspi.def");

        println!("cargo:rustc-link-arg=/FORCE:MULTIPLE");
        println!("cargo:rustc-link-arg=/DEF:{}", sspi_def_file.to_str().unwrap());
    }
}
