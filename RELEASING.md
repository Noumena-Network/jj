# Releasing Noumena jj

## Standards

- Versioning: SemVer 2.0.0
- Changelog: Keep a Changelog 1.1.0
- Builds: pure ecosystem-native builds only
- Platforms: Linux x64, macOS arm64, macOS x64
- Release trigger: tag reachable from main, matching `vX.Y.Z`

## Release Flow

1. Move `CHANGELOG.md` entries from `[Unreleased]` into a dated release section.
2. Update the project version source.
3. Open a PR and wait for CI to pass.
4. Merge to main.
5. Run a workflow-dispatch dry-run on main.
6. Create and push tag `vX.Y.Z` from main.
7. Let the release workflow publish archives, SHA256 sidecars, manifests, and attestations.

## Build Contract

This repository must not depend on the NCode Buck monorepo as its public build path.
