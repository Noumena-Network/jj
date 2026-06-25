# Noumena jj OSS Repo

## Build Contract

This public repo uses the ecosystem-native build path only.

- `gh`: pure Go, no Buck.
- `jj`: pure Cargo, no Buck.
- `sl`: pure Cargo, no Buck.

Do not add Buck, NCode monorepo wrappers, internal staging config, private cert paths, or generated release artifacts as public build requirements.

## Release Contract

- CI must build and smoke-test Linux x64, macOS arm64, and macOS x64.
- Public releases are tag-driven from `main` with dry-run support.
- Release assets must include archive, sha256 sidecar, manifest, and artifact attestations.
- Do not publish a release unless the workflow has been proven by a dry-run on the exact commit.

## Source Policy

Keep source minimal, but do not remove behavior to make the repo smaller. For extracted code, exclude files only when dependency metadata or tests prove they are outside the shipped CLI closure.
