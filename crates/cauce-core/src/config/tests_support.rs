//! Shared fixtures for the `config` module's colocated test modules: a
//! deterministic env map, a tempdir sandbox and a config-file writer.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::Path;

use super::EnvMap;

pub(super) fn env_of(pairs: &[(&str, &str)]) -> EnvMap {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

/// Temp config + data dirs, returned with an env map pointing at them.
pub(super) fn sandbox(extra: &[(&str, &str)]) -> (tempfile::TempDir, EnvMap) {
    let dir = tempfile::tempdir().unwrap();
    let mut map = env_of(&[
        ("CAUCE_CONFIG_DIR", dir.path().join("cfg").to_str().unwrap()),
        ("CAUCE_DATA_DIR", dir.path().join("data").to_str().unwrap()),
    ]);
    for (k, v) in extra {
        map.insert((*k).to_string(), (*v).to_string());
    }
    (dir, map)
}

pub(super) fn write_config(dir: &Path, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("config.toml"), body).unwrap();
}
