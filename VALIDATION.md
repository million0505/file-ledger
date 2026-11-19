# Validation

Checked on Intel macOS with Rust 1.99.1.

- `cargo test --locked`: 7 tests passed on macOS.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo build --release --locked`: passed.
- 8 end-to-end CLI checks passed:
  - scan writes a valid new manifest
  - duplicates use matching full content
  - unchanged verify exits zero
  - same-size content edit exits one
  - diff distinguishes added and removed paths
  - existing output is preserved
  - self-including manifest output is refused
  - traversal in supplied manifest is rejected

The Linux-specific non-UTF-8 filename fixture is not runnable on APFS. Linux and Windows have not been locally verified. No throughput benchmark or production acceptance is claimed.
