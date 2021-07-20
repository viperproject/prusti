// © 2021, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use cargo_test_support::{ProjectBuilder, cargo_test, project, symlink_supported};
use std::path::{Path, PathBuf};
use std::fs;

fn cargo_prusti_path() -> PathBuf {
    let target_directory = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let executable_name = if cfg!(windows) {
        "cargo-prusti.exe"
    } else {
        "cargo-prusti"
    };
    let local_prusti_rustc_path: PathBuf = ["target", target_directory, executable_name].iter().collect();
    if local_prusti_rustc_path.exists() {
        return fs::canonicalize(&local_prusti_rustc_path).expect(
            &format!("Failed to canonicalize the path {:?}", local_prusti_rustc_path)
        );
    }
    let workspace_prusti_rustc_path: PathBuf = ["..", "target", target_directory, executable_name].iter().collect();
    if workspace_prusti_rustc_path.exists() {
        return fs::canonicalize(&workspace_prusti_rustc_path).expect(
            &format!("Failed to canonicalize the path {:?}", workspace_prusti_rustc_path)
        );
    }
    panic!(
        "Could not find the {:?} cargo-prusti binary to be used in tests. \
        It might be that Prusti has not been compiled correctly.",
        target_directory
    );
}

fn link_prusti_crates_dep(mut project_builder: ProjectBuilder) -> ProjectBuilder {
    if let Some(prusti_dev_path) = project_builder.root().ancestors().find(|p| p.ends_with("prusti-dev")) {
        if let Ok(prusti_dev_path) = fs::canonicalize(prusti_dev_path) {
            let prusti_contract_deps = [
                "prusti-specs",
                "prusti-contracts",
                "prusti-contracts-impl",
                "prusti-contracts-internal",
            ];

            for crate_name in &prusti_contract_deps {
                project_builder = project_builder.symlink_dir(
                    prusti_dev_path.join(crate_name).as_path(),
                    &Path::new(crate_name)
                );
            }
        }
    }
    project_builder
}

#[cargo_test]
fn simple_assert_true() {
    let p = project()
        .file("src/main.rs", "fn main() { assert!(true); }")
        .build();
    p.process(cargo_prusti_path())
        .run();
}

#[cargo_test]
fn simple_assert_false() {
    let p = project()
        .file("src/main.rs", "fn main() { assert!(false); }")
        .build();
    p.process(cargo_prusti_path())
        .with_status(101)
        .with_stderr(
            "\
[CHECKING] foo v0.0.1 ([..])
[ERROR] [Prusti: verification error] the asserted expression might not hold
 --> src/main.rs:1:13
  |
1 | fn main() { assert!(false); }
  |             ^^^^^^^^^^^^^^^
  |
  = note: this error originates in the macro `assert` (in Nightly builds, run with -Z macro-backtrace for more info)

error: could not compile `foo` due to previous error
",
        )
        .run();
}

/// Test `cargo-prusti` on one of the crates in `test/cargo_verify`.
///
/// Special files and folders in the root of the test crate:
/// * `output.stdout` and `output.stderr`: if present, they are used to check the output of
///   `cargo-prusti`.
/// * `prusti-contracts` and related Prusti crates: during the test they will link to the
///   corresponding Prusti crate.
///
/// This function requires symlinks to be supported.
///
/// For more details on the special syntax allowed in the `output.*` files, check the documentation
/// of `cargo_test_support`: <https://doc.crates.io/contrib/tests/writing.html>.
fn test_local_project<T: Into<PathBuf>>(project_name: T) {
    let mut project_builder = project().no_manifest();

    let mut project_path = Path::new(&project_name.into()).to_path_buf();
    if project_path.is_relative() {
        project_path = fs::canonicalize(&Path::new("tests/cargo_verify").join(&project_path)).expect(&format!("Failed to canonicalize the path {}", project_path.display()))
    }

    // Populate the test project with symlinks to the local project
    let project_path_content = fs::read_dir(&project_path)
        .expect(&format!("Failed to read directory {}", project_path.display()));
    for entry in project_path_content {
        let entry = entry.expect(&format!("Failed to read content of {}", project_path.display()));
        let path = entry.path();
        let file_name = path.as_path().file_name()
            .expect(&format!("Failed to obtain the name of {}", path.display()));
        if path.is_dir() {
            project_builder = project_builder.symlink_dir(path.as_path(), &Path::new(file_name));
        } else {
            project_builder = project_builder.symlink(path.as_path(), &Path::new(file_name));
        }
    }

    // Create a special symlink for prusti_contract and related Prusti crates
    project_builder = link_prusti_crates_dep(project_builder);

    // symlink extern spec crate
    if let Ok(spec_path) = fs::canonicalize(&Path::new("tests/cargo_verify/extern_crate_tests/extern-spec-crate")) {
        project_builder = project_builder.symlink_dir(
           spec_path.as_path(), &Path::new("extern-spec-crate")
        );
    }

    // Fetch dependencies
    let project = project_builder.build();
    project.process("cargo").arg("build").run();

    // Set the expected exit status, stdout and stderr
    let mut test_builder = project.process(cargo_prusti_path());
    let opt_expected_stdout = fs::read_to_string(project_path.join("output.stdout")).ok();
    let opt_expected_stderr = fs::read_to_string(project_path.join("output.stderr")).ok();
    if let Some(ref expected_stdout) = opt_expected_stdout {
        test_builder.with_stdout(expected_stdout);
    }
    if let Some(ref expected_stderr) = opt_expected_stderr {
        test_builder.with_status(101).with_stderr(expected_stderr);
    }

    // Run the test
    test_builder.run();
}

#[cargo_test]
fn test_symlinks() {
    // Required by `test_local_project`
    assert!(symlink_supported());
}

#[cargo_test]
fn test_failing_crate() {
    test_local_project("failing_crate");
}

#[cargo_test]
fn test_prusti_toml() {
    test_local_project("prusti_toml");
}

#[cargo_test]
fn test_prusti_toml_fail() {
    let old_value = if let Ok(value) = std::env::var("RUST_BACKTRACE") {
        // We need to remove this environment variable because it affects the
        // compiler output.
        std::env::remove_var("RUST_BACKTRACE");
        Some(value)
    } else {
        None
    };
    test_local_project("prusti_toml_fail");
    if let Some(value) = old_value {
        std::env::set_var("RUST_BACKTRACE", value)
    }
}

// run all tests crates in extern_crate_tests
#[cargo_test]
fn test_all_extern_tests() {
    if let Ok(extern_tests_path) = fs::canonicalize(Path::new("tests/cargo_verify/extern_crate_tests")) {
        // run all tests that suppose to pass
        if let Ok(pass_dir) = fs::read_dir(&extern_tests_path.join("pass")) {
            for entry in pass_dir {
                let entry = entry.unwrap();
                test_local_project(entry.path());
            }
        }
        
        // run all tests that suppose to fail
        if let Ok(fail_dir) = fs::read_dir(&extern_tests_path.join("fail")) {
            for entry in fail_dir {
                let entry = entry.unwrap();
                test_local_project(entry.path());
            }
        }
    }
}
// TODO: automatically create a test for each folder in `test/cargo_verify`.
