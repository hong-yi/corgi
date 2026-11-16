//! Builds libghostty-vt (static) from vendor/ghostty with Zig, then generates
//! bindings with bindgen. Pins live in scripts/pins.env; run
//! scripts/bootstrap.sh first.
//!
//! This file is derived from herdr-core's crates/ghostty-vt/build.rs
//! (https://github.com/herdrdev/herdr-core), licensed under the Apache License 2.0
//! (see LICENSE-APACHE beside this crate's Cargo.toml), and stays under it; the
//! rest of corgi is MIT. Changed from the original: the Zig and Ghostty paths
//! (.tools/zig, ZIG, GHOSTTY_SRC), the CORGI_ZIG_CACHE cache folder, the
//! bindgen step, the xcframework always off, and the Windows PATH handling
//! removed.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..");
    let src = std::env::var_os("GHOSTTY_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("vendor/ghostty"));
    let src = src
        .canonicalize()
        .unwrap_or_else(|_| panic!("{} missing: run scripts/bootstrap.sh", src.display()));
    let out = PathBuf::from(env("OUT_DIR"));
    for path in ["src", "build.zig", "build.zig.zon", "include"] {
        println!("cargo:rerun-if-changed={}", src.join(path).display());
    }
    println!("cargo:rerun-if-env-changed=ZIG");
    println!("cargo:rerun-if-env-changed=GHOSTTY_SRC");
    println!("cargo:rerun-if-env-changed=CORGI_ZIG_CACHE");

    let zig = std::env::var_os("ZIG").map(PathBuf::from).unwrap_or_else(|| {
        let local = root
            .join(".tools/zig")
            .join(if cfg!(windows) { "zig.exe" } else { "zig" });
        if local.exists() { local } else { PathBuf::from("zig") }
    });

    let (target, host) = (env("TARGET"), env("HOST"));
    let windows = target.contains("windows");
    let mut build = Command::new(&zig);
    // Baseline CPU: release binaries run on machines other than the builder.
    // No xcframework: it needs xcodebuild and we only link the static archive.
    build.current_dir(&src).args([
        "build",
        "-Demit-lib-vt",
        "-Demit-xcframework=false",
        "-Doptimize=ReleaseFast",
        "-Dcpu=baseline",
    ]);
    if target != host || windows {
        build.arg(format!("-Dtarget={}", zig_target(&target)));
    }
    let status = build
        .arg("--prefix")
        .arg(out.join("zig-out"))
        // Zig's own build cache: in the build folder, or where CORGI_ZIG_CACHE says (CI keeps it
        // between runs: Ghostty's library comes from it in seconds instead of minutes).
        .arg("--cache-dir")
        .arg(std::env::var_os("CORGI_ZIG_CACHE").map_or_else(|| out.join("zig-cache"), PathBuf::from))
        .status()
        .unwrap_or_else(|e| panic!("cannot run {zig:?} (Zig 0.16 required; run scripts/bootstrap.sh): {e}"));
    assert!(status.success(), "zig build of libghostty-vt failed: {status}");

    // Only the static archive goes on the search path so the linker can't pick the dylib.
    let lib = out.join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let name = if windows {
        "ghostty-vt-static.lib"
    } else {
        "libghostty-vt.a"
    };
    std::fs::copy(out.join("zig-out/lib").join(name), lib.join(name)).unwrap();
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!(
        "cargo:rustc-link-lib=static={}",
        if windows { "ghostty-vt-static" } else { "ghostty-vt" }
    );

    bindings(&src.join("include"), &out.join("sys.rs"));
}

fn bindings(include: &Path, dest: &Path) {
    bindgen::Builder::default()
        .header(include.join("ghostty/vt.h").to_string_lossy())
        .clang_arg(format!("-I{}", include.display()))
        .clang_arg("-DGHOSTTY_STATIC")
        .allowlist_function("ghostty_.*")
        .allowlist_type("Ghostty.*")
        .allowlist_var("GHOSTTY_.*")
        .default_enum_style(bindgen::EnumVariation::Consts)
        .prepend_enum_name(false)
        .derive_default(true)
        .layout_tests(false)
        .generate_comments(false)
        .generate()
        .expect("bindgen failed on ghostty/vt.h")
        .write_to_file(dest)
        .expect("write bindings");
}

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("{key} not set"))
}

/// Cargo's target triple as Zig names it.
fn zig_target(triple: &str) -> String {
    let arch = triple.split('-').next().unwrap_or("native");
    let os = if triple.contains("apple") {
        "macos"
    } else if triple.contains("windows") {
        "windows"
    } else {
        "linux"
    };
    let abi = ["musl", "gnu", "msvc"].into_iter().find(|abi| triple.ends_with(abi));
    abi.map_or(format!("{arch}-{os}"), |abi| format!("{arch}-{os}-{abi}"))
}
