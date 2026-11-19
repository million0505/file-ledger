# File Ledger

A read-only Rust CLI for answering three questions about a folder:

- Which regular files are here, and what are their SHA-256 checksums?
- Which paths contain identical bytes?
- What changed since the last inventory?

It creates portable JSON manifests and machine-readable reports. It does not delete, rename, repair, upload, or deduplicate your files.

## Quick start

Requires a recent stable Rust toolchain. Tested locally with Rust 1.99.1 on Intel macOS.

```sh
cargo build --release --locked
./target/release/file-ledger --help
./target/release/file-ledger scan ./my-folder --out ./before.json
./target/release/file-ledger duplicates ./before.json
./target/release/file-ledger verify ./before.json ./my-folder
```

Save manifests **outside** the scanned folder. An existing output path is never overwritten. Use a new name for the next snapshot:

```sh
./target/release/file-ledger scan ./my-folder --out ./after.json
./target/release/file-ledger diff ./before.json ./after.json
```

All reports go to stdout as JSON. Diagnostics go to stderr.

| Exit | Meaning |
| --- | --- |
| 0 | Scan/report succeeded, or snapshots match |
| 1 | Comparison found added, removed, or changed files |
| 2 | Invalid input, I/O failure, or incomplete scan |

## What the reports mean

Each entry stores a relative UTF-8 path, byte count, and full SHA-256 digest. Paths are sorted. A same-size edit is still detected. Duplicate groups require matching size **and** checksum, not just a filename or file length.

A scan records skipped symlinks and special files separately from errors. If a snapshot has read errors, verification and duplicate reporting fail rather than treating unreadable files as missing or returning a misleading clean result. Root symlinks are rejected. Hidden regular files are included.

The manifest records file content, not permissions, ownership, extended attributes, empty directories, or link topology. Symlink changes are outside content comparison. Hard links may appear in duplicate groups; a duplicate report is not a promise of recoverable disk space.

## Design

- `src/scan.rs`: traversal and incremental hashing with a fixed 64 KiB read buffer.
- `src/model.rs`: versioned manifest types and path/digest validation.
- `src/lib.rs`: comparison, duplicate grouping, bounded loading, and atomic no-overwrite output publication.
- `src/main.rs`: command dispatch and exit codes.

File metadata is checked before and after reading to detect common concurrent edits. This is a content audit of ordinary local folders, not an atomic filesystem snapshot or a sandbox for hostile, concurrently modified directory trees. Avoid running against an actively changing tree when you need a stable inventory.

Manifests are limited to 64 MiB. File contents are streamed; path metadata and duplicate groups are held in memory. Atomic output publication uses a same-directory temporary file and a hard link; the destination filesystem must support hard links.

## Development

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

Tests cover a known SHA-256 vector, Unicode paths, equal-size different files, content changes, additions/removals, malformed paths/hashes, incomplete snapshots, no-overwrite output and symlink loops. A Linux-specific case covers non-UTF-8 filenames; APFS does not permit that fixture.

The `release/v0.1` branch is a handoff snapshot of this first working version. No external client work or performance benchmark is implied.

## License

MIT. See [LICENSE](LICENSE).
