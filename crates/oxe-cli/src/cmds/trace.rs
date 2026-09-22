//! `oxe trace <request_id>`: replay a request's span timeline from the
//! JSONL logs.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use oxe_core::config::Dirs;
use oxe_server::observability::{RequestId, trace};

/// `oxe trace <request_id>`. Returns the process exit code.
pub fn run(request_id: Option<String>) -> i32 {
    let Some(id) = request_id else {
        eprintln!("usage: oxe trace <request_id>");
        return 2;
    };
    if id.parse::<RequestId>().is_err() {
        eprintln!("oxe trace: {id:?} is not a UUID; expected `oxe trace <uuid>`");
        return 2;
    }
    // Same resolver `serve` writes logs through: OXE_DATA_DIR >
    // XDG_DATA_HOME > ~/.local/share/oxe.
    let dir = Dirs::detect().logs_dir();
    match trace::trace_request(&dir, &id) {
        Ok(records) => {
            print!("{}", trace::render_trace(&id, &records));
            0
        }
        Err(e) => {
            eprintln!("oxe trace: {e}");
            1
        }
    }
}
