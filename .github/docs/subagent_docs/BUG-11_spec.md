# BUG-11 — Disk I/O metrics zero on VMs and many real systems

## Current State Analysis

`crates/vexboard-server/src/metrics/system.rs::read_disk()` (lines 192-226)
parses `/proc/diskstats` and tries to keep only "whole disk" rows (skip
partitions) using a name-pattern heuristic:

- Rejects any device name ending in a digit unless it contains `"nvme"` or
  starts with `"sd"` (line 204-209).
- Then requires either `starts_with("sd") && len == 3` (i.e. exactly `sdX`)
  or `contains("nvme") && ends_with("n1") && !contains('p')` (line 211-212)
  to count as a whole disk.

This only matches classic SATA/SCSI (`sda`, `sdb`, ...) and one NVMe
namespace naming pattern. It excludes (reports zero I/O for) common real
device names:
- `vda`, `vdb`, ... (virtio, KVM/QEMU/Proxmox VMs)
- `xvda` (Xen, e.g. AWS Xen-based EC2 instances)
- `mmcblk0` (eMMC/SD card, e.g. Raspberry Pi)
- `md0`, `md1` (Linux software RAID)
- `dm-0`, `dm-1` (device-mapper: LVM, LUKS)
- `nvme0n2`, `nvme1n1`, etc. (multi-namespace or multi-drive NVMe — only
  `nvme0n1` specifically is matched due to the `starts_with`-style
  hardcoding is actually `contains("nvme")` — but breaks for `nvme1n1` too
  since `ends_with("n1")` requires literal namespace 1, and multi-digit
  device numbers like `nvme10n1` still pass, but device number is
  irrelevant; the real gap is namespaces other than `n1`)
- `sdaa`, `sdab`, ... (2-letter drive letters past `sdz`, `len() == 3`
  requires exactly 3 chars — `"sda"` — so `"sdaa"` (4 chars) fails)

On any of these systems, `read_disk()` silently returns `(0, 0)` for disk
I/O with no error or log — the dashboard just shows a permanently-zero disk
metric.

## Problem Definition

The whole-disk-vs-partition filter is a brittle, incomplete name-pattern
allowlist. It needs to be replaced with a mechanism that correctly
identifies "this line is a whole disk, not a partition" across all common
Linux block device naming schemes, without hardcoding a name pattern per
driver family.

## Proposed Solution

Linux already exposes the authoritative answer: **`/sys/block/<name>`
exists (as a directory) if and only if `<name>` is a whole/top-level block
device.** Partitions live under their parent's directory
(`/sys/block/sda/sda1`, `/sys/block/nvme0n1/nvme0n1p1`, ...) and are not
themselves top-level entries of `/sys/block`. This is the standard, kernel-
maintained way userspace tools (e.g. `lsblk`, `iostat`) distinguish whole
disks from partitions, and it covers every device family (`sd*`, `vd*`,
`xvd*`, `nvme*n*`, `mmcblk*`, `md*`, `dm-*`) uniformly with zero
per-family-name logic.

Implementation: read the entries of `/sys/block/` once per `read_disk()`
call into a `HashSet<String>`, then for each `/proc/diskstats` line, keep it
only if `parts[2]` is a member of that set. This replaces the entire
name-pattern block (lines 202-216) with a single set-membership check.

## Implementation Steps

1. In `read_disk()`, before the `for line in content.lines()` loop, read
   `/sys/block` directory entries via `tokio::fs::read_dir` into a
   `HashSet<String>` of device names. On error reading `/sys/block` (e.g.
   nonexistent on a non-Linux dev environment), fall back to an empty set
   — meaning no rows pass the filter, same "safe zero" behavior as today
   under abnormal conditions, not a new failure mode.
2. Replace lines 202-216 (`// Only count whole disk devices...` through the
   `is_whole_disk` check) with: `if !whole_disks.contains(name) { continue; }`.
3. Leave sector parsing (`parts[5]`, `parts[9]`) and the `* 512` byte
   conversion unchanged — the bug is only in device-name filtering.

## Dependencies

None — `tokio::fs::read_dir` is already used elsewhere via `tokio::fs`
(already a workspace dependency, `read_to_string` already imported in this
same function). No new crate, no Context7 lookup required.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** `/sys/block` unavailable (e.g. containerized/restricted
  environments, non-Linux dev machine).
  **Mitigation:** Treat read failure as "no known whole disks" → filter
  matches nothing → same zero-metric outcome as the current buggy behavior
  in the worst case, not worse. `/proc/diskstats` read itself already `?`s
  and propagates an error if unavailable, so total absence of Linux disk
  APIs already surfaces as an `Err`, unchanged by this fix.
- **Risk:** Reading `/sys/block` on every metrics tick (default every few
  seconds) is I/O overhead.
  **Mitigation:** It's a single small directory listing (typically <20
  entries), same order of cost as the `/proc/diskstats` read that already
  happens every tick — negligible, no caching added per Simplicity First.

## Approved Validation Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy -p vexboard-server -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`
