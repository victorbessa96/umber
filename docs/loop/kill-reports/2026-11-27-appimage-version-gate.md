# AppImage tool identity gate

- Lock: `razul-appimage-version-gate`.
- Brief: `7cb839d40de5` (`docs(ci): AppImage tool identity gate brief`).
- Outcome: the AppImage pack step now verifies the real binary identity instead of a string it never prints.

## Root cause and fix

Packaging run `45949526420` built `Umber-x86_64.AppImage` successfully, then failed at `./appimagetool.AppImage --version | grep -F 1.9.1`.

The exact 1.9.1 release asset reports `appimagetool, continuous build (git version 8c8c91f), build 296 built on 2025-12-04 17:55:56 UTC`. The workflow now:

- verifies SHA-256 `ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0` before execution;
- asserts `git version 8c8c91f` and `build 296` after packing;
- keeps those values and both tool URLs at one job scope so fetch and pack cannot drift apart.

## Verification

- Exact downloaded asset: `sha256sum -c -` returned `appimagetool.AppImage: OK`.
- Exact version command now requires one full identity line including the build timestamp; the real line passed and a mismatched line was refused.
- PyYAML and static workflow assertions: **AppImage identity gate GREEN**.
- `git diff --check`: clean.
- Independent review `deleg_866b62a8` returned SHIP and nonblocking hardening notes. Exact-line matching, local-recipe checksum, rolling-policy review date, Ubuntu retirement clock, and README scope were tightened. Final review `deleg_9c8cbd83`: **SHIP**, zero findings. The reviewer’s informational dead `APPIMAGETOOL_VERSION` note in `packaging.yml` was also removed.
- Commit/push/current-tip packaging CI: not yet claimed.

## Non-goals

No tag, release mutation, upstream PR, centralhost, or linuxdeploy/runtime policy change in this follow-up.
