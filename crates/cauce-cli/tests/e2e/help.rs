//! #89: `-h`/`--help` prints usage to stdout and exits 0 — it is a request
//! for help, not a usage error (previously every flag parser funnelled it
//! through the error path, so usage printed on stderr with exit 2).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use tokio::process::Command;

use crate::common::cauce_bin;

/// Every subcommand answers `-h`/`--help` with usage on stdout and exit 0.
#[tokio::test]
async fn help_flags_exit_zero() {
    #[allow(clippy::vec_init_then_push)]
    let mut cases = vec![
        ("serve", "-h"),
        ("serve", "--help"),
        ("record", "--help"),
        ("engine", "-h"),
        ("engine", "test"),
        ("tail", "--help"),
    ];
    // `cauce mcp` exists only in `mcp` builds (W1-12).
    #[cfg(feature = "mcp")]
    cases.push(("mcp", "-h"));
    for (sub, flag) in cases {
        let mut cmd = Command::new(cauce_bin());
        cmd.arg(sub);
        // `engine test -h` exercises the flag loop; the rest are the
        // first-arg forms.
        if flag == "test" {
            cmd.arg("test").arg("-h");
        } else {
            cmd.arg(flag);
        }
        let output = cmd.output().await.expect("spawn cauce");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(0),
            "cauce {sub} {flag}: exit {:?}, stderr: {stderr}",
            output.status.code()
        );
        assert!(
            stdout.contains("usage: cauce"),
            "cauce {sub} {flag}: stdout missing usage: {stdout}"
        );
    }
}

/// A genuinely unknown flag still exits 2 on stderr.
#[tokio::test]
async fn unknown_flag_still_errors() {
    let output = Command::new(cauce_bin())
        .args(["serve", "--bogus"])
        .output()
        .await
        .expect("spawn cauce");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown flag"), "stderr: {stderr}");
}
