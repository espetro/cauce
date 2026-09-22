# later: sqlite-vec ANN for tier 3
Issue: #68

Seam: `Store::get_semantic` (brute-force over `cache_vec`, see `semantic-tier.md`).
Trigger: a deployment exceeds ~50k cached queries and tier-3 lookup p95 crosses 20 ms.
Shape: `sqlite-vec` statically linked (cargo feature `semantic-ann`), `vec0` virtual table
mirroring `cache_vec`, same threshold semantics; SQLite stays the only file.
