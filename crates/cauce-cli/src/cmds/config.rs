//! `cauce config <show|path>`: inspect the resolved configuration.
//!
//! `show` prints the effective config as TOML with secrets redacted
//! (`${env:...}`/`${file:...}` templates print as-is; resolved secret values
//! never). `path` prints the config file location and works even when the
//! file is missing or malformed.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::config::{Config, Dirs};

const USAGE: &str = "usage: cauce config <show|path>";

/// Entry point for the `config` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("show") => show(),
        Some("path") => {
            println!("{}", Dirs::detect().config_file().display());
            0
        }
        _ => {
            eprintln!("{USAGE}");
            2
        }
    }
}

fn show() -> i32 {
    match Config::load().and_then(|cfg| cfg.display_toml()) {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(e) => {
            eprintln!("cauce config show: {e}");
            2
        }
    }
}
