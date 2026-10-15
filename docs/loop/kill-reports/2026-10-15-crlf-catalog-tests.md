# Kill report — CRLF broke a catalog test on Windows CI (979a523)

Date: 2026-10-15. Author: dragon. Lock: `crlf-fix` over
`crates/umber-app/src/{palette, rich_tooltip}.rs`, `.gitattributes` (new)
and this report. Fixes master CI run 38745917721 (979a523, the command
palette): `windows-latest` red, `ubuntu-latest` green.

## Cause

`palette::tests::every_catalog_key_is_in_both_catalogs...` read the
`.ftl` catalogs with `include_str!` and split them on `"\n{group} =\n"`.
`actions/checkout` on Windows runs with git's default `core.autocrlf =
true`, so the checked-out catalogs had CRLF line endings. The pattern
never matched, `.nth(1)` was `None`, and the test panicked at "no `menu`
block". The build host and the laptop are Linux (LF), so every local
battery was green. The new rich-tooltip test had the same shape and would
have failed the same way on its first Windows run.

The two older catalog tests (status_bar's and navigation's hint keys)
use `contains(".attr =")`, which matches under either line ending. They
are left as they are.

## Built

- **The two split-based tests now ask the real Fluent bundles**
  (`I18n::builtin().has_key(locale, key)`). This is the same lookup the
  app does, so the tests check what users get, with no parsing of their
  own.
- The rich-tooltip "no hard-coded shortcut in tip.*" guard still reads
  the raw text (it inspects strings, not lookups), now normalised with
  `.replace("\r\n", "\n")`.
- **`.gitattributes` (new):** `eol=lf` for `*.rs *.ftl *.toml *.md *.wgsl
  *.json *.sh`. Every tracked text file is already LF in the index
  (`git ls-files --eol`: 0 non-LF), so nothing changes in history. A
  Windows checkout now keeps LF too, which also makes the per-file
  sha256 checks in the umber-wave-ops verify step platform-independent.

## Lesson (recorded in umber-wave-ops)

A Linux battery cannot see a line-ending bug. Tests that parse an
embedded text asset must use the real parser or normalise line endings,
and the repo pins `eol=lf` for the asset types it embeds.
