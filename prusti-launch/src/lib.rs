// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![deny(unused_must_use)]

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};
use serde::Deserialize;
#[cfg(target_family = "unix")]
use nix::{sys::signal::{Signal, killpg}, unistd::getpgrp};

/// Append paths to the loader environment variable
pub fn add_to_loader_path(paths: Vec<PathBuf>, cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    const LOADER_PATH: &str = "PATH";
    #[cfg(target_os = "linux")]
    const LOADER_PATH: &str = "LD_LIBRARY_PATH";
    #[cfg(target_os = "macos")]
    const LOADER_PATH: &str = "DYLD_FALLBACK_LIBRARY_PATH";
    env_prepend_path(LOADER_PATH, paths, cmd);
}

/// Prepend paths to an environment variable
fn env_prepend_path(name: &str, value: Vec<PathBuf>, cmd: &mut Command) {
    let old_value = env::var_os(name);
    let mut parts: Vec<PathBuf>;
    if let Some(ref v) = old_value {
        parts = value;
        parts.extend(env::split_paths(v).collect::<Vec<_>>());
    } else {
        parts = value;
    }
    match env::join_paths(parts) {
        Ok(new_value) => {
            cmd.env(name, new_value);
        }
        Err(err) => panic!("Error: {:?}", err),
    }
}

/// Find the sub-folder containing the JVM dynamic library
pub fn find_libjvm<S: AsRef<Path>>(path: S) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    const EXPECTED_JVM_FILENAME: &str = "jvm.dll";
    #[cfg(target_os = "linux")]
    const EXPECTED_JVM_FILENAME: &str = "libjvm.so";
    // On MacOS, we need to link to libjli instead of libjvm as a workaround to a Java8 bug.
    // See: https://github.com/jni-rs/jni-rs/pull/230
    #[cfg(target_os = "macos")]
    const EXPECTED_JVM_FILENAME: &str = "libjli.dylib";

    let walker = walkdir::WalkDir::new(path).follow_links(true);

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_e) => continue,
        };

        let file_name = entry.file_name().to_str().unwrap_or("");

        if file_name == EXPECTED_JVM_FILENAME {
            return entry.path().parent().map(Into::into);
        }
    }

    None
}

/// Find the Java home directory
pub fn find_java_home() -> Option<PathBuf> {
    Command::new("java")
        .arg("-XshowSettings:properties")
        .arg("-version")
        .output()
        .ok()
        .and_then(|output| {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            for line in stdout.lines().chain(stderr.lines()) {
                if line.contains("java.home") {
                    let pos = line.find('=').unwrap() + 1;
                    let path = line[pos..].trim();
                    return Some(PathBuf::from(path));
                }
            }
            None
        })
}

pub fn get_rust_toolchain_channel() -> String {
    #[derive(Deserialize)]
    struct RustToolchainFile {
        toolchain: RustToolchain,
    }

    #[derive(Deserialize)]
    struct RustToolchain {
        channel: String,
        #[allow(dead_code)]
        components: Option<Vec<String>>,
    }

    let content = include_str!("../../rust-toolchain");
    // Be ready to accept TOML format
    // See: https://github.com/rust-lang/rustup/pull/2438
    if content.starts_with("[toolchain]") {
        let rust_toolchain: RustToolchainFile = toml::from_str(content)
            .expect("failed to parse rust-toolchain file");
        rust_toolchain.toolchain.channel
    } else {
        content.trim().to_string()
    }
}

/// Find Prusti's sysroot
pub fn prusti_sysroot() -> Option<PathBuf> {
    match env::var("RUST_SYSROOT") {
        Ok(prusti_sysroot) => Some(PathBuf::from(prusti_sysroot)),
        Err(_) => get_sysroot_from_rustup()
    }
}

fn get_sysroot_from_rustup() -> Option<PathBuf> {
    Command::new("rustup")
        .arg("run")
        .arg(get_rust_toolchain_channel())
        .arg("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .ok()
        .and_then(|out| {
            print!("{}", String::from_utf8(out.stderr).ok().unwrap());
            String::from_utf8(out.stdout).ok()
        })
        .map(|s| PathBuf::from(s.trim().to_owned()))
}

/// Find Viper home
pub fn find_viper_home(base_dir: &PathBuf) -> Option<PathBuf> {
    let candidates = vec![
        base_dir.join("viper_tools").join("server"),
        base_dir.join("..").join("..").join("viper_tools").join("server"),
        base_dir.join("viper_tools").join("backends"),
        base_dir.join("..").join("..").join("viper_tools").join("backends"),
    ];

    for candidate in candidates.into_iter() {
        if candidate.is_dir() {
            return Some(candidate);
        }
    }

    None
}

/// Find Z3 executable
pub fn find_z3_exe(base_dir: &PathBuf) -> Option<PathBuf> {
    let mut candidates = vec![
        base_dir.join("viper_tools").join("z3").join("bin").join("z3"),
        base_dir.join("..").join("..").join("viper_tools").join("z3").join("bin").join("z3"),
    ];

    if cfg!(windows) {
        candidates.iter_mut().for_each(|x| { x.set_extension("exe"); });
    }

    for candidate in candidates.into_iter() {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

#[cfg(target_family = "unix")]
pub fn sigint_handler() {
    // Killing the process group terminates the process tree
    killpg(getpgrp(), Signal::SIGKILL).expect("Error killing process tree.");
}

#[cfg(target_family = "windows")]
pub fn sigint_handler() {
    use std::process::{self, Stdio};

    // Kill process tree rooted at prusti-server.exe
    let pid: &str = &*process::id().to_string();

    Command::new("TASKKILL")
        .args(&["/PID", pid, "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Error killing process tree.");
}
