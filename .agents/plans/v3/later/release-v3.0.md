# later: v3.0 release checklist
Issue: #66

Trigger: W6-01 accepted (last v3.0 step).
- cargo-dist or `cargo build --release` matrix (macOS arm64/x86_64, Linux x86_64/arm64 musl)
- Homebrew tap formula, `cargo install oxe`, GitHub release with checksums
- `docs/` reviewed against the routes table and modes; `~/SEARCH.md` final form
- CHANGELOG from Conventional Commits since the orphan commit
- retro note in `.agents/notes/` comparing v3 delivery against the wave dates
