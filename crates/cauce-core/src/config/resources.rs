//! Host resource detection (`sysinfo`) and the resource-adaptive defaults
//! the settled inputs require — never a fixed large allocation.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::store::StoreTuning;

/// Host resources detected at startup; the resource-adaptive defaults the
/// settled inputs require (never a fixed large allocation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resources {
    /// SQLite `cache_size`/`mmap_size`/`busy_timeout` tuning (W0-03 type,
    /// consumed by `cauce-store-sqlite`).
    pub store_tuning: StoreTuning,
    /// Max concurrent upstream engine calls, scaled to cores.
    pub upstream_concurrency: u16,
    /// `cargo test`/`nextest` thread budget, scaled to cores.
    pub test_threads: u16,
}

impl Resources {
    /// Read available memory and core count via `sysinfo`, then derive.
    pub fn detect() -> Self {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        sys.refresh_cpu_all();
        let mem_bytes = sys.available_memory();
        let cores = sysinfo::System::physical_core_count()
            .unwrap_or_else(|| sys.cpus().len())
            .min(usize::from(u16::MAX)) as u16;
        Self::from_specs(mem_bytes, cores)
    }

    /// Pure derivation from injected numbers (the acceptance test path).
    ///
    /// Scaling rules, clamped so small machines stay usable and big ones do
    /// not get an unbounded allocation:
    ///
    /// - `cache_size_kib`: RAM / 128, clamped to 8-512 MiB
    ///   (4 GiB -> 32 MiB, 32 GiB -> 256 MiB).
    /// - `mmap_size_bytes`: RAM / 16, clamped to 64 MiB-4 GiB
    ///   (4 GiB -> 256 MiB, 32 GiB -> 2 GiB).
    /// - `busy_timeout_ms`: fixed 5000.
    /// - `upstream_concurrency`: 4 per core, clamped to 4-64.
    /// - `test_threads`: one per core, clamped to 1-32.
    pub fn from_specs(mem_bytes: u64, cores: u16) -> Self {
        const KIB: u64 = 1024;
        const MIB: u64 = 1024 * KIB;
        const GIB: u64 = 1024 * MIB;
        let cache_size_kib = (mem_bytes / 128 / KIB).clamp(8 * KIB, 512 * KIB) as u32;
        let mmap_size_bytes = (mem_bytes / 16).clamp(64 * MIB, 4 * GIB);
        Self {
            store_tuning: StoreTuning {
                cache_size_kib,
                mmap_size_bytes,
                busy_timeout_ms: 5_000,
            },
            upstream_concurrency: (u32::from(cores) * 4).clamp(4, 64) as u16,
            test_threads: cores.clamp(1, 32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn resources_scale_with_memory() {
        let small = Resources::from_specs(4 * GIB, 8);
        let big = Resources::from_specs(32 * GIB, 8);
        assert!(small.store_tuning.cache_size_kib < big.store_tuning.cache_size_kib);
        assert!(small.store_tuning.mmap_size_bytes < big.store_tuning.mmap_size_bytes);
        assert_eq!(small.store_tuning.busy_timeout_ms, 5_000);
    }

    #[test]
    fn resources_clamp_extremes() {
        let tiny = Resources::from_specs(512 * 1024 * 1024, 1);
        assert_eq!(tiny.store_tuning.cache_size_kib, 8 * 1024);
        assert_eq!(tiny.test_threads, 1);
        assert_eq!(tiny.upstream_concurrency, 4);

        let huge = Resources::from_specs(1024 * GIB, 128);
        assert_eq!(huge.store_tuning.cache_size_kib, 512 * 1024);
        assert_eq!(huge.store_tuning.mmap_size_bytes, 4 * GIB);
        assert_eq!(huge.upstream_concurrency, 64);
        assert_eq!(huge.test_threads, 32);
    }
}
