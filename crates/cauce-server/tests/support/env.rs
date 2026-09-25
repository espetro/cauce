//! Process-env discipline for `CAUCE_*`-reading tests: `env_lock`
//! serialises mutation, `config_env` writes a sandbox `config.toml` and
//! points the env at it, `clear_env` drops the vars back.

use std::sync::OnceLock;

/// Serialises tests that mutate process env (`CAUCE_CONFIG_DIR` and
/// friends). Under nextest each test is its own process anyway; this
/// keeps plain `cargo test` (one process per test binary) safe too.
static ENV_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

#[allow(dead_code)]
pub async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// Write `toml_src` to `tmp/cfg/config.toml` and point the process env at
/// the sandbox. Caller must hold `env_lock`.
#[allow(dead_code)]
pub fn config_env(toml_src: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg_dir = tmp.path().join("cfg");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(cfg_dir.join("config.toml"), toml_src).unwrap();
    // SAFETY: serialized by ENV_LOCK; nextest also isolates per process.
    unsafe {
        std::env::set_var("CAUCE_CONFIG_DIR", &cfg_dir);
        std::env::set_var("CAUCE_DATA_DIR", tmp.path().join("data"));
    }
    tmp
}

/// Clear the vars `config_env` and the config tests may set. Caller holds
/// the lock.
#[allow(dead_code)]
pub fn clear_env() {
    // SAFETY: serialized by ENV_LOCK; nextest also isolates per process.
    unsafe {
        std::env::remove_var("CAUCE_CONFIG_DIR");
        std::env::remove_var("CAUCE_DATA_DIR");
        std::env::remove_var("CAUCE_SEARCH_DEADLINE_MS");
        std::env::remove_var("CAUCE_SEARCH_TTL_S");
        std::env::remove_var("CAUCE_ADMISSION_MAX_WAIT_MS");
        std::env::remove_var("CAUCE_ADMISSION_MAX_CONCURRENT_PER_ENGINE");
        std::env::remove_var("CAUCE_LOGS_RETENTION_DAYS");
        std::env::remove_var("CAUCE_AI_BASE_URL");
        std::env::remove_var("CAUCE_AI_API_KEY");
        std::env::remove_var("CAUCE_AI_MODEL");
        std::env::remove_var("CAUCE_AI_ENABLED");
        std::env::remove_var("CAUCE_ENGINES");
        std::env::remove_var("CAUCE_EVAL_RESULTS_DIR");
        std::env::remove_var("PROVIDER_API_KEY");
    }
}
