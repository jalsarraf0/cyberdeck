# CI/CD Hardening Report -- cyberdeck

**Repository:** `jalsarraf0/cyberdeck`
**Date:** 2026-03-14
**Branch:** `ci/assurance-hardening`

---

## Pre-Existing CI/CD Infrastructure

cyberdeck had three workflows in place before hardening:

| Workflow | Status | Notes |
|---|---|---|
| CI (`ci.yml`) | Operational | Cross-platform test matrix (Linux/macOS/Windows), Docker regression |
| Security (`security.yml`) | Partial | cargo-audit + clippy, push/PR/schedule triggers, no secret scanning |
| Release (`release.yml`) | Operational | Multi-platform build, attestation, tag verification, GitHub Release |

### What Was Missing

- No `deny.toml` or cargo-deny policy enforcement
- No Gitleaks secret scanning
- No concurrency controls on any workflow
- No SARIF upload for security findings
- No ASSURANCE.md or hardening documentation

---

## What Was Added

| Item | Type | Description |
|---|---|---|
| `deny.toml` | New file | cargo-deny policy: deny unmaintained/unsound/yanked, license allowlist, deny unknown registries/git |
| cargo-deny job in `security.yml` | New CI job | Runs `cargo deny check` on every push, PR, and weekly schedule |
| Gitleaks job in `security.yml` | New CI job | Full-history secret scan with SARIF upload to GitHub Security tab |
| Concurrency controls in `security.yml` | Workflow enhancement | `security-${{ github.ref }}` group with cancel-in-progress |
| Permissions block in `security.yml` | Workflow enhancement | Least-privilege: `contents: read`, `security-events: write` |
| Concurrency controls in `ci.yml` | Workflow enhancement | `ci-${{ github.ref }}` group with cancel-in-progress |
| `ASSURANCE.md` | New file | Comprehensive software assurance document |
| `CI_CD_HARDENING_REPORT.md` | New file | This report |

---

## What Was NOT Changed

- `release.yml` was not modified (already has attestation, tag verification, and proper permissions)
- No source code was modified
- Existing Security jobs (dependency-audit, static-analysis) were preserved unchanged
- README.md badges already covered CI and Security workflows

---

## Verification

All changes are structural (YAML workflow definitions, TOML policy, Markdown docs).
Syntax validation:

- `deny.toml`: valid TOML, matches cargo-deny schema
- `security.yml`: valid GitHub Actions YAML, preserves existing jobs
- `ci.yml`: valid GitHub Actions YAML (concurrency block added)

---

## Remaining Recommendations

| Item | Priority | Notes |
|---|---|---|
| CodeQL workflow | Medium | Add `codeql.yml` for Rust semantic analysis (SSH-Hunt model) |
| SBOM generation | Medium | Add CycloneDX SBOM workflow for source-level bill of materials |
| Trivy filesystem scan | Low | Additional vulnerability scanning layer |
| OSV-Scanner | Low | Google OSV database cross-reference |
| Release badge in README | Low | Add Release workflow badge alongside existing CI and Security badges |
