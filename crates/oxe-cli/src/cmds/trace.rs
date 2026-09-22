//! `oxe trace <request_id>`: replay a request's span timeline from the
//! JSONL logs.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use oxe_server::observability::{RequestId, logs_dir, trace};

/// `oxe trace <request_id>`.
pub fn run(request_id: Option<String>) {
    let Some(id) = request_id else {
        eprintln!("usage: oxe trace <request_id>");
        std::process::exit(2);
    };
    if id.parse::<RequestId>().is_err() {
        eprintln!("oxe trace: {id:?} is not a UUID; expected `oxe trace <uuid>`");
    }
    let dir = logs_dir();
    match trace::trace_request(&dir, &id) {
        Ok(records) => print!("{}", trace::render_trace(&id, &records)),
        Err(e) => {
            eprintln!("oxe trace: {e}");
            std::process::exit(1);
        }
    }
}
