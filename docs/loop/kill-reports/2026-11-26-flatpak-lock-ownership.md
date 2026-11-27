# Flatpak source/lockfile ownership

- Lock: `razul-flatpak-lock-ownership`.
- Brief: `5719ed1e5d4b` (`docs(flatpak): source lock ownership brief`).
- Outcome: source pin and registry closure now come from one commit, and the real drift check passes.

## Root cause and fix

The manifest pinned `v0.1.0-alpha.1` / `bc4d642f`, but the generator read the working tree's newer Cargo.lock. That paired one source with another commit's vendor closure and made `--check` fail.

`source_context` now resolves:

- Development/manual: the template's pinned git commit and that commit's `Cargo.lock`.
- `refs/tags/...`: `GITHUB_REF_NAME` and `GITHUB_SHA` replace the source tag and commit, and the same SHA supplies the lockfile. The updated source is re-read and asserted equal to the release SHA.

The git reader is injectable for exact unit tests and fails closed on invalid commits or unreadable lockfiles. Tag names are JSON-escaped. `render` returns the manifest and crate count together, avoiding duplicate source resolution. The lint checkout uses full history so an ancestor pin is readable. Existing archive entries and the committed alpha manifest remain byte-identical.

## Verification

- `python3 flatpak/test_gen_cargo_sources.py`: **5/5 passed** for development ownership, non-tag SHA behavior, tag ownership, quoted-tag JSON safety, and invalid SHA refusal. Lock content is keyed to the requested commit.
- Real `python3 flatpak/gen-cargo-sources.py --check`: **PASS** on the current repo.
- Python compile, JSON parse, and `git diff --check`: clean.
- Independent re-review `sa-0-f4a4d22c`: **SHIP**, all six findings resolved and verified in full-history and depth-1 tag clones.
- Commit/push/current-tip CI: not yet claimed. Tag-native invocation is present in `release.yml`; actual native build evidence remains post-push.

## Non-goals

No workflow dispatch, tag, release mutation, upstream PR, centralhost, or Flatpak runtime change in this slice.
