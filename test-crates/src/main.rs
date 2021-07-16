// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rustwide::{
    cmd,
    Crate,
    Toolchain,
    WorkspaceBuilder,
    logging,
    logging::LogStorage,
};
use serde::Deserialize;
use log::{info, warn, error, LevelFilter};

#[derive(Debug, Deserialize)]
struct CrateRecord {
    name: String,
    version: String,
}

impl From<CrateRecord> for Crate {
    fn from(record: CrateRecord) -> Self {
        Crate::crates_io(&record.name, &record.version)
    }
}

fn setup_logs() {
    let mut env = env_logger::Builder::new();
    env.filter_module("test_crates", log::LevelFilter::Info);
    if let Ok(content) = std::env::var("TEST_CRATES_LOG") {
        env.parse_filters(&content);
    }
    rustwide::logging::init_with(env.build());
}

struct CargoPrusti {
    prusti_home: PathBuf,
    viper_home: PathBuf,
    z3_exe: PathBuf,
    java_home: Option<PathBuf>,
}

impl cmd::Runnable for CargoPrusti {
    fn name(&self) -> cmd::Binary {
        cmd::Binary::Global(self.prusti_home.join("cargo-prusti"))
    }

    fn prepare_command<'w, 'pl>(&self, cmd: cmd::Command<'w, 'pl>) -> cmd::Command<'w, 'pl> {
        let java_home = self.java_home.as_ref()
            .map(|p| p.to_str().unwrap())
            .unwrap_or("/usr/lib/jvm/default-java");
        cmd.env("VIPER_HOME", self.viper_home.to_str().unwrap())
            .env("Z3_EXE", self.z3_exe.join("z3").to_str().unwrap())
            .env("JAVA_HOME", java_home)
            .env("CARGO_PATH", "/opt/rustwide/cargo-home/bin/cargo")
    }
}

#[derive(Deserialize)]
struct RustToolchainFile {
    toolchain: RustToolchain,
}

#[derive(Deserialize)]
struct RustToolchain {
    channel: String,
    components: Option<Vec<String>>,
}

fn get_rust_toolchain() -> RustToolchain {
    let content = include_str!("../../rust-toolchain");
    let rust_toolchain: RustToolchainFile = toml::from_str(content)
        .expect("failed to parse rust-toolchain file");
    rust_toolchain.toolchain
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

fn main() -> Result<(), Box<dyn Error>> {
    color_backtrace::install();
    setup_logs();

    let workspace_path = Path::new("../workspaces/test-crates-builder");
    let host_prusti_home = if cfg!(debug_assertions) {
        Path::new("target/debug")
    } else {
        Path::new("target/release")
    };
    let host_viper_home = Path::new("viper_tools/server");
    let host_z3_home = Path::new("viper_tools/z3/bin");
    let host_java_home = env::var("JAVA_HOME").ok().map(|s| s.into())
        .or_else(find_java_home)
        .expect("Please set JAVA_HOME");
    let guest_prusti_home = Path::new("/opt/rustwide/prusti-home");
    let guest_viper_home = Path::new("/opt/rustwide/viper-home");
    let guest_z3_home = Path::new("/opt/rustwide/z3-home");
    let guest_java_home = Path::new("/opt/rustwide/java-home");

    info!("Using host's Java home {:?}", host_java_home);
    let cargo_prusti = CargoPrusti {
        prusti_home: guest_prusti_home.to_path_buf(),
        viper_home: guest_viper_home.to_path_buf(),
        z3_exe: guest_z3_home.to_path_buf(),
        java_home: Some(guest_java_home.to_path_buf()),
    };

    info!("Crate a new workspace...");
    let workspace = WorkspaceBuilder::new(
        workspace_path,
        "prusti-test-crates"
    ).init()?;

    info!("Install the toolchain...");
    let rust_toolchain = get_rust_toolchain();
    info!("toolchain: {}", rust_toolchain.channel);
    let toolchain = Toolchain::dist(&rust_toolchain.channel);
    toolchain.install(&workspace)?;
    for component in rust_toolchain.components.as_ref().unwrap_or(&vec![]).iter() {
        if component != "rustfmt" {
            toolchain.add_component(&workspace, component)?;
        }
    }

    info!("Read lists of crates...");
    // TODO: do something to freeze the version of the dependencies.
    let crates_list: Vec<Crate> = csv::Reader::from_reader(
       fs::File::open("test-crates/no_panic.csv")?
    ).deserialize()
        .collect::<Result<Vec<CrateRecord>, _>>()?
        .into_iter()
        .map(|c| c.into())
        // For the moment, test only a few of the crates.
        .take(500)
        .collect();

    // List of crates that don't compile with the standard compiler.
    let mut skipped_crates = vec![];
    // List of crates on which Prusti fails.
    let mut failed_crates = vec![];
    // List of crates on which Prusti succeed.
    let mut successful_crates = vec![];

    info!("Iterate over all {} crates...", crates_list.len());
    for (index, krate) in crates_list.iter().enumerate() {
        info!("Crate {}/{}: {}", index + 1, crates_list.len(), krate);

        info!("Fetch crate...");
        krate.fetch(&workspace)?;

        info!("Build crate...");
        {
            let mut build_dir = workspace.build_dir(&format!("build_{}", index));

            let sandbox = cmd::SandboxBuilder::new()
                .memory_limit(Some(1024 * 1024 * 1024))
                .enable_networking(false);

            let mut storage = LogStorage::new(LevelFilter::Info);
            storage.set_max_size(1024 * 1024);

            let build_status =  build_dir.build(&toolchain, &krate, sandbox).run(|build| {
                logging::capture(&storage, || {
                    build.cargo()
                        .args(&["check"])
                        .run()?;
                    Ok(())
                })
            });

            if let Err(err) = build_status {
                warn!("Error: {:?}", err);
                warn!("Output:\n{}", storage);

                // Do not try to verify this crate
                skipped_crates.push(krate);
                continue;
            }
        }

        info!("Verify crate...");
        {
            let mut build_dir = workspace.build_dir(&format!("verify_{}", index));

            let sandbox = cmd::SandboxBuilder::new()
                .memory_limit(Some(1024 * 1024 * 1024))
                .enable_networking(false)
                .mount(&host_prusti_home, &guest_prusti_home, cmd::MountKind::ReadOnly)
                .mount(&host_viper_home, &guest_viper_home, cmd::MountKind::ReadOnly)
                .mount(&host_z3_home, &guest_z3_home, cmd::MountKind::ReadOnly)
                .mount(&host_java_home, &guest_java_home, cmd::MountKind::ReadOnly);

            let mut storage = LogStorage::new(LevelFilter::Info);
            storage.set_max_size(1024 * 1024);

            let verification_status = build_dir.build(&toolchain, &krate, sandbox).run(|build| {
                logging::capture(&storage, || {
                    build.cmd(&cargo_prusti)
                        .env("RUST_BACKTRACE", "1")
                        .env("PRUSTI_ASSERT_TIMEOUT", "60000")
                        .env("PRUSTI_CHECK_PANICS", "false")
                        // Do not report errors for unsupported language features
                        .env("PRUSTI_SKIP_UNSUPPORTED_FEATURES", "true")
                        .env("PRUSTI_LOG_DIR", "/tmp/prusti_log")
                        .run()?;
                    Ok(())
                })
            });

            if let Err(err) = verification_status {
                error!("Error: {:?}", err);
                error!("Output:\n{}", storage);

                // Report the failure
                failed_crates.push(krate);
            } else {
                successful_crates.push(krate);
            }
        }
    }

    // Report summary
    if !successful_crates.is_empty() {
        info!("Successfully verified {} crates:", successful_crates.len());
        for krate in &successful_crates {
            info!(" - {}", krate);
        }
    }
    if !skipped_crates.is_empty() {
        warn!("Skipped {} crates:", skipped_crates.len());
        for krate in &skipped_crates {
            warn!(" - {}", krate);
        }
    }
    if !failed_crates.is_empty() {
        error!("Failed to verify {} crates:", failed_crates.len());
        for krate in &failed_crates {
            error!(" - {}", krate);
        }
    }

    // Panic
    if !failed_crates.is_empty() {
        panic!("Failed to verify {} crates", failed_crates.len());
    }

    Ok(())
}
