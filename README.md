# Pixel1HLE (`pixel1-translator`)

Experimental **High-Level Emulation** research project targeting **Google Pixel 1 (sailfish)** / **Android 7.1**-era native packages.

This README reflects **verified source reality**, not aspirations.

## What works today (verified in tree)

| Area | Status | Notes |
|------|--------|--------|
| Desktop GUI shell (egui) | Partial | Window + AppPicker / boot / loading UI states exist |
| APK ZIP inspection | **Implemented + unit tested** | `apk_inspect::inspect_apk` lists ABIs, `.so` names, manifest presence |
| ELF parse / anonymous map | Partial | `object` + `memmap2` map section bytes; **not** full PT_LOAD program-header loader |
| RELA application | **Not implemented** | `apply_relocations` only records undefined symbols / stubs; **does not patch GOT** |
| Guest AArch64 execution on x86_64/Windows | **Unsupported** | `JNI_OnLoad` is **not** invoked on non-`aarch64` hosts (would be UB) |
| JNI ABI / ART | Missing | No real `JavaVM` / `JNIEnv` |
| Syscall translator | Stub table | Returns success / `-ENOSYS`; not wired to a CPU frontend |
| GLES bridge | Host stubs | Some `gl*` calls forward to `glow` when a context exists; EGL is largely fake |
| Android APK product | CI packaging | NativeActivity shell; not a working emulator of arbitrary APKs |

## What does **not** work

- Running arbitrary Android games or Unity/AIR titles to completion on Windows/Linux x86_64.
- Transparent execution of ARM64 machine code on x86 hosts without a CPU emulator / DBT backend.
- Full Bionic, Binder, or framework compatibility.

## Build

```bash
# Library + desktop binary
cargo check --features desktop,loader
cargo test --features desktop,loader
cargo build --release --features desktop,loader
```

Android NDK / APK packaging is handled by `.github/workflows` and the `android/` Gradle tree. Treat those as packaging experiments until the core loader/execution path is real.

## Architecture (current)

```
APK file
  → apk_inspect (ZIP metadata)          [works, tested]
  → ApkElfExecutor extract .so          [works]
  → map bytes + symbol inventory        [partial]
  → apply_relocations                   [NO GOT writes yet]
  → call JNI_OnLoad                     [blocked on non-aarch64]
  → CPU / syscall frontend              [missing]
```

## Honest next milestones

1. **PT_LOAD + real RELA** application with bounds checks and unit tests.
2. **Guest execution model**: integrate an AArch64 interpreter/DBT **or** restrict runtime to aarch64 hosts.
3. Minimal **JNIEnv** surface after execution exists.
4. Graphics only after guest code can call into the GLES bridge safely.

## License

MIT
