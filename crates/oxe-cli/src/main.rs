//! `oxe` binary stub. Subcommands land in later wave-0 steps (serve in
//! W0-09/W0-12, the rest per the wave files).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

fn main() {
    let sub = std::env::args().nth(1).unwrap_or_default();
    match sub.as_str() {
        "serve" => eprintln!("oxe serve: not implemented yet"),
        _ => eprintln!("usage: oxe <serve|search|engine|cache|record|trace|config> [args]"),
    }
}
