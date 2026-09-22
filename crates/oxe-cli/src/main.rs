//! `oxe` binary: `serve`, `mcp`, `record`, `engine`, `trace`, `tail`,
//! `config` are implemented; `search` and `cache` land in later waves.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod cmds;

fn main() {
    let sub = std::env::args().nth(1).unwrap_or_default();
    match sub.as_str() {
        "serve" => std::process::exit(cmds::serve::run(
            &std::env::args().skip(2).collect::<Vec<_>>(),
        )),
        "mcp" => std::process::exit(cmds::mcp::run(
            &std::env::args().skip(2).collect::<Vec<_>>(),
        )),
        "record" => std::process::exit(cmds::record::run(
            &std::env::args().skip(2).collect::<Vec<_>>(),
        )),
        "engine" => std::process::exit(cmds::engine::run(
            &std::env::args().skip(2).collect::<Vec<_>>(),
        )),
        "trace" => std::process::exit(cmds::trace::run(std::env::args().nth(2))),
        "tail" => std::process::exit(cmds::tail::run(
            &std::env::args().skip(2).collect::<Vec<_>>(),
        )),
        "config" => std::process::exit(cmds::config::run(
            &std::env::args().skip(2).collect::<Vec<_>>(),
        )),
        // Declared in the plan but not implemented yet: say so instead of
        // falling into generic usage.
        "search" | "cache" => {
            eprintln!("oxe {sub}: not implemented yet; lands in a later wave");
            std::process::exit(2);
        }
        _ => {
            eprintln!("usage: oxe <serve|mcp|record|engine|trace|tail|config> [args]");
            std::process::exit(2);
        }
    }
}
