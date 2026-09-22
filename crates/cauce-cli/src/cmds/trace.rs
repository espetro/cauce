//! `cauce trace <request_id>`: replay a request's span timeline from the
//! JSONL logs.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::config::Dirs;
use cauce_server::observability::{RequestId, trace};

/// `cauce trace <request_id>`. Returns the process exit code.
pub fn run(request_id: Option<String>) -> i32 {
    let Some(id) = request_id else {
        eprintln!("usage: cauce trace <request_id>");
        return 2;
    };
    if id.parse::<RequestId>().is_err() {
        eprintln!("cauce trace: {id:?} is not a UUID; expected `cauce trace <uuid>`");
        return 2;
    }
    // Same resolver `serve` writes logs through: CAUCE_DATA_DIR >
    // XDG_DATA_HOME > ~/.local/share/cauce.
    let dir = Dirs::detect().logs_dir();
    match trace::trace_request(&dir, &id) {
        Ok(records) => {
            print!("{}", trace::render_trace(&id, &records));
            0
        }
        Err(e) => {
            eprintln!("cauce trace: {e}");
            1
        }
    }
}
