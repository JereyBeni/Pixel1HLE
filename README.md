# pixel1-translator

**High-Level Emulation (HLE)** for Android 7.1 (Pixel 1 / sailfish) with a full **dynamic linker** and **OpenGL ES → Desktop GL** bridge.

## Architecture

```
APK (lib/arm64-v8a/libunity.so)
         │
         ▼
┌─────────────────────────────────────────────┐
│  ApkElfExecutor  (HLE linker)               │
│  - unzip lib/arm64-v8a/*.so                 │
│  - PT_LOAD → RWX host mapping               │
│  - RELA patching                            │
│  - symbol redirect → gles_bridge            │
│  - JNI_OnLoad with mocked JavaVM            │
└────────────────────┬────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────┐
│  GLES Bridge (extern "C")                   │
│  egl* / gl*  →  glow::Context (GL 4.5)      │
└────────────────────┬────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────┐
│  HardwareStack + Binder + SyscallTranslator │
└─────────────────────────────────────────────┘
```

## Features

| Feature | Status |
|---------|--------|
| GLES/EGL HLE bridge | ✅ |
| Global glow context | ✅ |
| APK extraction (zip) | ✅ |
| PT_LOAD mapping + mprotect | ✅ |
| Symbol resolver (EGL/GLES/bionic) | ✅ |
| JNI_OnLoad bootstrap | ✅ |
| Graceful missing-symbol stubs | ✅ |

## Quick start

```bash
cargo run --release
```

To test the full linker path, place any APK that contains `lib/arm64-v8a/libunity.so` (or another engine) as `demo.apk` next to the binary:

```bash
cp /path/to/your.apk demo.apk
cargo run --release
```

## Feature flag

```bash
cargo check --features loader
```

## Status

Production-grade skeleton. The linker will load real ARM64 `.so` files, map them, and call `JNI_OnLoad`. Unresolved symbols fall back to safe stubs instead of crashing.
