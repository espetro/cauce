//! `cauce tail [--follow] [--request <id>] [--level warn] [--engine bing]
//! [-n LINES]`: pretty-print the JSONL logs, one line per event, spans
//! collapsed (`engine=bing 640ms ok`).
//!
//! Reads the newest `cauce-YYYY-MM-DD.jsonl` under `logs/` (last `-n` lines,
//! 50 by default); `--follow` keeps polling for appended lines and rolls
//! over to a newer file at daily rotation. Filtering and rendering live in
//! `cauce_server::observability::tail`; colour is on when stdout is a TTY and
//! `NO_COLOR` is unset.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::fs::{self, File};
use std::io::{IsTerminal, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use cauce_core::config::Dirs;
use cauce_server::observability::tail::{self, Tail, TailFilter};
use cauce_server::observability::trace::{self, LogRecord};

const USAGE: &str = "usage: cauce tail [--follow] [--request <id>] [--level <lvl>] \
                     [--engine <id>] [-n <lines>]";

/// Poll interval for `--follow`.
const POLL: Duration = Duration::from_millis(300);

struct TailOpts {
    /// `--follow`/`-f`: keep printing appended lines.
    follow: bool,
    /// `--request <id>`: request-id prefix filter.
    request: Option<String>,
    /// `--level <lvl>`: minimum severity.
    level: Option<String>,
    /// `--engine <id>`: engine filter.
    engine: Option<String>,
    /// `-n`/`--lines`: how many lines of the newest file to start from.
    lines: usize,
}

/// Entry point for the `tail` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let opts = match parse(args) {
        Ok(opts) => opts,
        Err(msg) => {
            eprintln!("cauce tail: {msg}\n{USAGE}");
            return 2;
        }
    };
    let dir = Dirs::detect().logs_dir();
    let files = match trace::log_files(&dir) {
        Ok(files) if !files.is_empty() => files,
        _ => {
            eprintln!(
                "cauce tail: no JSONL logs under {} (is `cauce serve` running?)",
                dir.display()
            );
            return 1;
        }
    };
    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let mut tail = Tail::new(
        TailFilter {
            request: opts.request,
            level: opts.level,
            engine: opts.engine,
        },
        color,
    );

    let mut path = files[0].clone();
    let mut offset = match initial_read(&path, opts.lines, &mut tail) {
        Ok(offset) => offset,
        Err(e) => {
            eprintln!("cauce tail: cannot read {}: {e}", path.display());
            return 1;
        }
    };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    if !opts.follow {
        for line in tail.finish() {
            let _ = writeln!(out, "{line}");
        }
        return 0;
    }

    loop {
        // Daily rotation: switch to the newest file once it appears, after a
        // final drain of the current one.
        if let Ok(files) = trace::log_files(&dir)
            && let Some(newest) = files.first()
            && *newest != path
        {
            let _ = drain(&path, &mut offset, &mut tail, &mut out);
            path = newest.clone();
            offset = 0;
        }
        match drain(&path, &mut offset, &mut tail, &mut out) {
            Ok(0) => std::thread::sleep(POLL),
            Ok(_) => {}
            Err(e) => {
                eprintln!("cauce tail: {}: {e}", path.display());
                return 1;
            }
        }
        let _ = out.flush();
    }
}

/// Print the last `lines` records of `path`; returns the byte offset to
/// start following from. An unterminated trailing line (a write still in
/// flight) is neither printed nor consumed: the offset stops at the last
/// newline so the next `drain` re-reads it once complete.
fn initial_read(path: &Path, lines: usize, tail: &mut Tail) -> std::io::Result<u64> {
    let content = fs::read_to_string(path)?;
    let complete = content.rfind('\n').map_or(0, |i| i + 1);
    let all: Vec<&str> = content[..complete].lines().collect();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in &all[all.len().saturating_sub(lines)..] {
        emit(tail, line, &mut out);
    }
    let _ = out.flush();
    Ok(complete as u64)
}

/// Emit one JSONL line through the renderer.
fn emit(tail: &mut Tail, line: &str, out: &mut impl Write) {
    let Ok(record) = serde_json::from_str::<LogRecord>(line) else {
        return;
    };
    if let Some(rendered) = tail.push(&record) {
        let _ = writeln!(out, "{rendered}");
    }
}

/// Read complete lines appended to `path` since `offset`; returns how many
/// bytes were consumed. A truncated/recreated file restarts at 0.
fn drain(
    path: &PathBuf,
    offset: &mut u64,
    tail: &mut Tail,
    out: &mut impl Write,
) -> std::io::Result<u64> {
    let mut file = File::open(path)?;
    if file.metadata()?.len() < *offset {
        *offset = 0;
    }
    file.seek(SeekFrom::Start(*offset))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    // Only whole lines: keep the trailing partial line for the next poll.
    let complete = buf.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    for line in buf[..complete].split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        emit(tail, &String::from_utf8_lossy(line), out);
    }
    *offset += complete as u64;
    Ok(complete as u64)
}

fn parse(args: &[String]) -> Result<TailOpts, String> {
    let mut opts = TailOpts {
        follow: false,
        request: None,
        level: None,
        engine: None,
        lines: 50,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) => (f, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| it.next().cloned())
                .ok_or_else(|| format!("missing value for {name}"))
        };
        match flag {
            "--follow" | "-f" => opts.follow = true,
            "--request" => opts.request = Some(value(flag)?),
            "--level" => {
                let raw = value(flag)?;
                if !tail::valid_level(&raw) {
                    return Err(format!(
                        "invalid --level {raw:?}; expected trace|debug|info|warn|error"
                    ));
                }
                opts.level = Some(raw);
            }
            "--engine" => opts.engine = Some(value(flag)?),
            "--lines" | "-n" => {
                let raw = value(flag)?;
                opts.lines = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --lines {raw:?}"))?;
            }
            "-h" | "--help" => return Err("help requested".into()),
            other => return Err(format!("unknown flag {other:?}")),
        }
    }
    Ok(opts)
}
