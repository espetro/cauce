# later: semantic cache tier (tier 3)
Issue: #58, #59

Seam: `Store::get_semantic` / `put_embedding` and the `cache_vec` table; both stay in the
trait and schema as unimplemented stubs.
Trigger: tier-2 FTS5 + Jaccard measurably misses paraphrase duplicates the owner cares
about, or the AI answer loop wants semantic recall over the archive.
Shape: `fastembed` with an INT8 small model (`bge-small-en-v1.5` quantised or
`all-MiniLM-L6-v2`), cargo feature `semantic`, `cache.semantic.enabled` default false;
resource-gated load (skip when available memory < ~512 MB at startup, log why); model file
cached under `$OXE_DATA_DIR/models/`; brute-force cosine over `cache_vec` for < 50k rows
(`later/sqlite-vec-ann.md` covers the ANN follow-up); hit threshold cosine >= 0.92, same
page/lang; query embedding computed only on tier-1 and tier-2 miss. Must not change
tier-1/tier-2 lookup semantics.
