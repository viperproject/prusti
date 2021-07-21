// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![feature(custom_test_frameworks)]
// Custom test runner, to avoid libtest being wrapped around compiletest which
// wraps libtest.
#![test_runner(test_runner)]

extern crate compiletest_rs;
extern crate prusti_server;

use compiletest_rs::{common::Mode, run_tests, Config};
use prusti_server::ServerSideService;
use std::{env, path::PathBuf};

fn find_prusti_rustc_path() -> PathBuf {
    let target_directory = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let executable_name = if cfg!(windows) {
        "prusti-rustc.exe"
    } else {
        "prusti-rustc"
    };
    let local_prusti_rustc_path: PathBuf = ["target", target_directory, executable_name].iter().collect();
    if local_prusti_rustc_path.exists() {
        return local_prusti_rustc_path;
    }
    let workspace_prusti_rustc_path: PathBuf = ["..", "target", target_directory, executable_name].iter().collect();
    if workspace_prusti_rustc_path.exists() {
        return workspace_prusti_rustc_path;
    }
    panic!(
        "Could not find the {:?} prusti-rustc binary to be used in tests. \
        It might be that Prusti has not been compiled correctly.",
        target_directory
    );
}

/// This type allows to temporary modify an environment variable.
/// When this structure is dropped (falls out of scope), the original value will be restored.
struct TemporaryEnvVar {
    name: String,
    original: Option<String>,
}

impl TemporaryEnvVar {
    /// Temporarily set an environment variable, until the returned value is dropped.
    fn set(name: &str, value: &str) -> Self {
        let original: Option<String> = env::var(name).ok();
        env::set_var(name, value);
        TemporaryEnvVar {
            name: name.to_string(),
            original,
        }
    }
}

impl Drop for TemporaryEnvVar {
    fn drop(&mut self) {
        match self.original {
            Some(ref old_value) => env::set_var(&self.name, old_value),
            None => env::remove_var(&self.name),
        }
    }
}

fn run_prusti_tests(group_name: &str, filter: &Option<String>, rustc_flags: Option<&str>) {
    let mut config = Config::default();
    config.rustc_path = find_prusti_rustc_path();

    // Filter the tests to run
    if let Some(filter) = filter {
        config.filters.push(filter.clone());
    }

    // Add compilation flags
    config.target_rustcflags = Some(format!(
        "--edition=2018 {}",
        rustc_flags.unwrap_or("")
    ));

    let path: PathBuf = ["tests", group_name, "ui"].iter().collect();
    if path.exists() {
        config.target_rustcflags = Some(format!(
            "--color=never {}",
            config.target_rustcflags.unwrap_or("".to_string())
        ));
        config.mode = Mode::Ui;
        config.src_base = path;
        run_tests(&config);
    }

    let path: PathBuf = ["tests", group_name, "pass"].iter().collect();
    if path.exists() {
        config.mode = Mode::RunPass;
        config.src_base = path;
        run_tests(&config);
    }

    let path: PathBuf = ["tests", group_name, "fail"].iter().collect();
    if path.exists() {
        config.mode = Mode::CompileFail;
        config.src_base = path;
        run_tests(&config);
    }

    // Delete the nll-facts directory to avoid running out of hard drive
    // space. Ignore any errors that may occur.
    let _ = std::fs::remove_dir_all("nll-facts");
    let _ = std::fs::remove_dir_all("log/nll-facts");
}

fn run_no_verification(group_name: &str, filter: &Option<String>) {
    let _temporary_env_vars = (
        TemporaryEnvVar::set("PRUSTI_FULL_COMPILATION", "true"),
        TemporaryEnvVar::set("PRUSTI_NO_VERIFY", "true"),
        TemporaryEnvVar::set("PRUSTI_QUIET", "true"),
    );

    run_prusti_tests(group_name, filter, None);
}

fn run_verification(group_name: &str, filter: &Option<String>) {
    let _temporary_env_vars = (
        TemporaryEnvVar::set("PRUSTI_FULL_COMPILATION", "true"),
        TemporaryEnvVar::set("PRUSTI_ENCODE_UNSIGNED_NUM_CONSTRAINT", "true"),
        TemporaryEnvVar::set("PRUSTI_QUIET", "true"),
        TemporaryEnvVar::set("PRUSTI_CHECK_OVERFLOWS", "false"),
    );

    run_prusti_tests(group_name, filter, Some("-A warnings"));
}

fn run_verification_overflow(group_name: &str, filter: &Option<String>) {
    let _temporary_env_vars = (
        TemporaryEnvVar::set("PRUSTI_CHECK_OVERFLOWS", "true"),
    );

    run_verification(group_name, filter);
}

fn run_verification_core_proof(group_name: &str, filter: &Option<String>) {
    let _temporary_env_vars = (
        TemporaryEnvVar::set("PRUSTI_CHECK_PANICS", "false"),
        TemporaryEnvVar::set("PRUSTI_CHECK_OVERFLOWS", "false"),
    );

    run_verification(group_name, filter);
}

fn test_runner(_tests: &[&()]) {
    // Spawn server process as child (so it stays around until main function terminates)
    let server_address = ServerSideService::spawn_off_thread();
    env::set_var("PRUSTI_SERVER_ADDRESS", server_address.to_string());

    // Filter the tests to run
    let filter = env::args().nth(1);

    // Test the parsing of specifications. This doesn't run the verifier.
    println!("[parse]");
    run_no_verification("parse", &filter);

    // Test the type-checking of specifications. This doesn't run the verifier.
    println!("[typecheck]");
    run_no_verification("typecheck", &filter);

    // Test the verifier.
    println!("[verify]");
    run_verification("verify", &filter);

    // Test the verifier with overflow checks enabled.
    println!("[verify_overflow]");
    run_verification_overflow("verify_overflow", &filter);

    // Test the verifier with panic checks disabled (i.e. verify only the core proof).
    println!("[core_proof]");
    run_verification_core_proof("core_proof", &filter);
}
