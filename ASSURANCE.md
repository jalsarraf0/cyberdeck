# Software Assurance

This document describes the CI/CD gates, security scanning, supply chain protections,
and quality controls that guard the cyberdeck codebase. Every pull request and push to
`main` must pass these gates before code is merged or released.

---

## Table of Contents

- [CI/CD Pipeline Overview](#cicd-pipeline-overview)
- [Code Quality Gates](#code-quality-gates)
- [Security Scanning](#security-scanning)
- [Supply Chain Security](#supply-chain-security)
- [Concurrency and Runner Controls](#concurrency-and-runner-controls)
- [Running Checks Locally](#running-checks-locally)
- [How This Protects Against Regressions](#how-this-protects-against-regressions)

---

## CI/CD Pipeline Overview

cyberdeck uses three GitHub Actions workflows that collectively enforce code quality,
security posture, and supply chain integrity:

| Workflow | File | Trigger | Purpose |
|---|---|---|---|
| **CI** | `ci.yml` | push, PR | Cross-platform test matrix, Docker regression |
| **Security** | `security.yml` | push, PR, weekly schedule, manual | Vulnerability scanning, secret detection, license/policy enforcement, static analysis |
| **Release** | `release.yml` | `v*` tags | Multi-platform build, attestation, GitHub Release publishing |

Linux jobs run on self-hosted runners (`[self-hosted, Linux, X64, docker]`).
Cross-platform CI also tests on `macos-latest` and `windows-latest`.

---

## Code Quality Gates

### Formatting

`cargo fmt --all -- --check` enforces consistent Rust formatting. PRs with formatting
violations are rejected.

### Linting

`cargo clippy --workspace --all-targets -- -D warnings` treats every Clippy warning as
a hard error. Runs in both the CI regression job and the Security static analysis job.

### Testing

- **Cross-platform tests**: `cargo test --all-targets` runs on Linux (self-hosted),
  macOS, and Windows.
- **Docker regression**: The CI workflow runs a full regression suite inside a Docker
  container including fmt, clippy, tests, and `scripts/regression.sh`.

### Release Verification

The release workflow verifies formatting, tests, and full regression before building
release artifacts. It also validates that the Git tag version matches `Cargo.toml`.

---

## Security Scanning

### cargo-audit

Checks `Cargo.lock` against the RustSec Advisory Database for known vulnerabilities in
dependencies. Runs on every push, PR, and weekly schedule.

### cargo-deny

Enforces policy rules defined in `deny.toml`:

- **Advisories**: denies unmaintained, unsound, and yanked crates.
- **Bans**: warns on duplicate dependency versions.
- **Licenses**: rejects dependencies with incompatible licenses; allows common OSS licenses
  (MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib, MPL-2.0,
  CC0-1.0, OpenSSL, BSL-1.0).
- **Sources**: denies unknown registries and unknown Git sources.

### Clippy Static Analysis

`cargo clippy --all-targets -- -D warnings` runs as a dedicated Security workflow job,
providing a second layer of static analysis beyond the CI regression check.

### Gitleaks

Scans the full Git history for accidentally committed secrets (API keys, tokens,
passwords, private keys). Results are uploaded as SARIF to the GitHub Security tab.

---

## Supply Chain Security

### Build Provenance Attestation

`actions/attest-build-provenance` generates SLSA-compatible provenance attestations for
every release artifact (Linux, macOS, Windows). Attestations provide an auditable record
of what was built, when, and by which workflow run.

### Locked Dependencies

Release builds use `--locked` to ensure the exact dependency versions from `Cargo.lock`
are used. Any dependency drift between local development and CI is caught immediately.

---

## Concurrency and Runner Controls

### Concurrency Groups

Each workflow uses concurrency groups scoped to `${{ github.ref }}` with
`cancel-in-progress: true`. This ensures that:

- Redundant runs on the same branch are cancelled automatically.
- CI resources are not wasted on superseded commits.
- Results always reflect the latest pushed state.

### Least-Privilege Permissions

Every workflow declares explicit `permissions` blocks scoped to the minimum required:

- CI: `contents: read`
- Security: `contents: read`, `security-events: write`
- Release: `contents: write`, `id-token: write`, `attestations: write`

### Runner Selection

Linux jobs run on self-hosted runners (`[self-hosted, Linux, X64, docker]`).
Cross-platform CI jobs use GitHub-hosted `macos-latest` and `windows-latest` runners.

---

## Running Checks Locally

Contributors can run the same checks that CI enforces before pushing:

```bash
# Formatting
cargo fmt --all --check

# Linting
cargo clippy --workspace --all-targets -- -D warnings

# Full test suite
cargo test --all-targets

# Full Docker regression
./scripts/regression.sh

# Dependency audit
cargo audit

# License and policy check
cargo deny check --config deny.toml --hide-inclusion-graph bans licenses sources

# Secret scan (requires gitleaks)
gitleaks detect --source . --redact --verbose
```

---

## How This Protects Against Regressions

| Risk | Mitigation |
|---|---|
| Formatting drift | `cargo fmt --check` on every PR |
| Lint regressions | `clippy -D warnings` in CI and Security workflows |
| Test failures | Cross-platform tests + Docker regression on every PR |
| Known CVEs in dependencies | cargo-audit on every PR + weekly |
| License violations | cargo-deny policy enforcement on every PR |
| Leaked secrets | Gitleaks full-history scan on every PR |
| Dependency version drift | `--locked` flag on release builds |
| Wasted CI resources | Concurrency groups with cancel-in-progress |
| Over-privileged workflows | Explicit least-privilege permission blocks |
| Tampered release artifacts | Build provenance attestation |
| Unmaintained/unsound dependencies | cargo-deny advisory policy on every PR |
| Cross-platform breakage | Test matrix covering Linux, macOS, Windows |
