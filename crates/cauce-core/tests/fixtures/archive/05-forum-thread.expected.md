**wren42** · Member · posted 2025-08-02 14:12

My monthly scrub just reported `uncorrectable_errors: 2` on `/dev/sda`, but `smartctl -a` shows a completely clean bill — zero reallocations, zero pending sectors, zero CRC errors. The filesystem still mounts and reads fine. Do I panic-buy a new SSD tonight or is this a known scrub quirk?

System: kernel 6.10.3, btrfs-progs 6.9, single-device profile on a 2 TB SATA SSD about 18 months old.

**iodine** · Moderator · posted 2025-08-02 15:40

> Do I panic-buy a new SSD tonight or is this a known scrub quirk?

Neither, probably. First check whether the errors are in metadata or data: `btrfs scrub status -R /mount/point`. If it is metadata and you run `DUP` or RAID1 metadata, the good copy already healed it. The counter is cumulative — `btrfs scrub status` keeps showing the last run until the next scrub finishes.

**wren42** · Member · posted 2025-08-02 16:05

That was it — `scrub status -R` shows the two errors were in `metadata_bytes`, and the counter says `corrected: 2` for DUP. I had read the summary line and panicked. Re-ran the scrub this morning; zero errors. SMART stays clean.

Lesson learned: "uncorrectable" in the summary means "uncorrectable at read time", and the `-R` breakdown is what you actually want to look at. Marking solved.

**iodine** · Moderator · posted 2025-08-02 16:11

For the record: if it ever is *data* errors on a single-device profile, `btrfs restore` plus a file-level scrub is the recovery path — but schedule a replacement anyway, since a growing error count usually precedes visible failure by weeks, not years.