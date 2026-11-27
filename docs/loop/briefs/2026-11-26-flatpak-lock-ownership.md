# Kill brief: Flatpak source/lockfile ownership

- Lock: `razul-flatpak-lock-ownership`.
- Trigger: platform audit PLAT-02 showed that the manifest pins one source commit while the generator reads a different working-tree Cargo.lock, making lint red and threatening vendor-checksum mismatch.
- Scope: `gen-cargo-sources.py`, direct unit tests, and the kill report. Release-workflow invocation is a follow-up edit after the active release-graph lane lands.

## Required behavior

- Development/manual generation reads the manifest's pinned git `commit` and renders from **that commit's** `Cargo.lock`, not the working tree's.
- When `GITHUB_REF` is `refs/tags/...`, replace the template's tag and commit with `GITHUB_REF_NAME`/`GITHUB_SHA`, render from that SHA's lockfile, and assert the resulting source commit equals the release SHA.
- Preserve the existing archive entries and committed alpha manifest byte-for-byte. Fail closed if the pinned commit or its lockfile cannot be read.

## Tests

- Injected git reader proves development uses the pinned commit, tag context rewrites source pin + lock source to the same SHA, and the alpha source's registry entries remain unchanged.
- Real `python3 flatpak/gen-cargo-sources.py --check` must pass on the current repo.
- Python compile, JSON parse, `git diff --check`, and independent review before commit.

## Constraints

No workflow dispatch, tag, release mutation, upstream PR, centralhost, or Flatpak runtime change. No release-workflow edit in this bounded slice.
