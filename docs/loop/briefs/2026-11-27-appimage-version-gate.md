# Kill brief: AppImage tool identity gate

- Lock: `razul-appimage-version-gate`.
- Trigger: packaging run `45949526420` built `Umber-x86_64.AppImage` successfully, then failed at `./appimagetool.AppImage --version | grep -F 1.9.1`.

## Root cause and required fix

The exact release asset at `https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage` reports `appimagetool, continuous build (git version 8c8c91f), build 296 built on 2025-12-04 17:55:56 UTC`; it never prints `1.9.1`.

- Verify the downloaded binary against SHA-256 `ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0` before execution.
- Assert the actual git version `8c8c91f` and build `296` after packing instead of grepping the release-tag string.
- Document the release tag, reported identity, and checksum in `appimage/README.md`.

## Verification and constraints

Local execution of the exact asset must confirm the identity and checksum; YAML/static checks and independent review precede commit. The next packaging run is the real gate. No tag, release mutation, upstream PR, or centralhost work.
