Released 2025-10-01 · [Download](https://example.com/download/2.9) · [Upgrade guide](https://example.com/docs/upgrading)

## Highlights

*   Streaming compaction reduces write amplification by up to 40% on append-heavy workloads.
*   Prepared statements are now cached per connection, cutting parse overhead on repeated queries.
*   The `VACUUM ANALYZE` command gains a `PARALLEL n` option (default 1).

## Breaking changes

*   `hb_ctl backup --incremental` now requires a prior full backup manifest in the target directory; it fails with exit code 3 otherwise.
*   The deprecated `max_row_size` setting was removed; use `max_tuple_bytes` instead.
*   Clients older than protocol 3.2 are rejected at handshake with a clear error instead of timing out.

## Fixed

*   Index-only scans no longer return stale visibility on hot standby after a replay burst (#4821).
*   Corrected a race where two concurrent `CREATE INDEX CONCURRENTLY` calls on the same table could silently drop one index (#4877).
*   JSON path queries on deeply nested arrays no longer overflow the evaluation stack (#4902).

## Deprecation notices

The `legacy_hash` index type is deprecated and scheduled for removal in 3.2. Migration guidance is in the [upgrade guide](https://example.com/docs/migrating-legacy-hash).

## Checksums

```
harbingerdb-2.9-linux-x86_64.tar.gz  sha256: 9f2c…a1
harbingerdb-2.9-darwin-arm64.tar.gz  sha256: 77e4…be
```