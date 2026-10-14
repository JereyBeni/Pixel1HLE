---
name: Bug Report
about: Report a crash, graphical glitch, or linker failure in Pixel1HLE
title: "[BUG] "
labels: bug
assignees: ''
---

## Description
<!-- Clear and concise description of what went wrong -->

## Steps to Reproduce
1.
2.
3.

## Expected Behaviour

## Actual Behaviour

## Platform Verification (required)

Please check **exactly one** of the environments where the issue occurred:

- [ ] **Windows (desktop EXE package)**
  - Did you unpack the release zip so the layout looks like this?
    ```
    pixel1-translator.exe
    config/pixel1-sailfish.yaml
    apps/          ← guest APKs go here
    fonts/
    assets/
    ```
  - Path of the APK you tried to load (relative to `apps/` or full path from **Import ya!**):
    ```
    
    ```
  - Windows version (e.g. 10 22H2 / 11 23H2):

- [ ] **Android (sandboxed runtime)**
  - Confirm the HLE is reading/writing only under:
    ```
    Android/data/com.pixel.hle/
    ├── config/
    ├── apps/
    ├── fonts/
    ├── assets/
    ├── cache/
    └── logs/
    ```
  - Did you inspect directory permissions on `Android/data/com.pixel.hle/`?
    - [ ] Yes – permissions look correct
    - [ ] No / not sure
  - Android version & device (e.g. Pixel 6, Android 14):

## Logs
<!-- Paste the relevant part of the console / logcat output. Look for targets hle::linker, hle::gles, hle::display -->

```

```

## Screenshots / Video (optional)

## Additional Context
- Pixel1HLE version / commit / release tag:
- Guest APK name & package (if known):
- Any non-default config changes in `pixel1-sailfish.yaml`:
