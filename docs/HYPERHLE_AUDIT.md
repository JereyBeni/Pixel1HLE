# HyperHLE → Pixel1HLE audit

Reference: `https://github.com/KlugKlugTG/HyperHLE-Fork` (`src/`, ~542 paths under `src/`).
Target: `https://github.com/JereyBeni/Pixel1HLE`.

HyperHLE (touchHLE lineage) is an **iOS app emulator** hosted on desktop/Android.
It is **not** an Android application runtime. Pixel1HLE targets **Android APKs / ELF / JNI**.

License note: HyperHLE sources are **MPL-2.0**. Do **not** copy implementation files into Pixel1HLE; reuse **architecture ideas** only.

## Inventory (high-level groups)

| Area | HyperHLE paths | Actual responsibility | iOS-specific? |
|------|----------------|----------------------|---------------|
| CPU | `cpu.rs`, `cpu/dynarmic_wrapper/*` | Dynarmic DBT for **ARMv6/v7 32-bit**; memory callbacks into `Mem` | ISA is ARM32 iPhone OS; pattern of DBT is portable |
| Memory | `mem.rs`, `mem/allocator.rs`, `mem/host.rs` | Guest address space, safe read/write, null page | Generic pattern |
| ABI | `abi.rs` | Guest function pointers, host↔guest calls | AAPCS variants; Android uses AAPCS64 |
| Loader | `mach_o.rs`, `dyld.rs`, `dyld/*` | **Mach-O** + Apple dyld semantics | **Exclude** as code; reimplement for **ELF** |
| ObjC | `objc/*`, many `frameworks/*` | Objective-C runtime + UIKit/Foundation HLE | **Exclude** |
| GLES | `gles.rs`, `gles/*` | GLES translation / host GL | Conceptually reusable |
| FS / bundle | `fs.rs`, `bundle.rs` | App bundle layout, sandboxed FS | IPA vs **APK** |
| Android host | `android_jni.rs`, `android_media.rs`, `android_web_view.rs` | Hosting the **iOS emulator on Android** | Inverse of Pixel1HLE’s goal |
| Window / input | `window.rs`, UIKit input | Host window | Adapt host side only |
| Libc | `libc/*`, `libc/mach/*` | Darwin/Bionic-ish shims for iOS apps | Reimplement Bionic surface |
| Environment | `environment.rs` | Threads, mutexes, app lifecycle | Pattern reusable |

Full path list is 500+ files; frameworks alone dominate. Anything under `frameworks/` and `objc/` is **iPhone OS HLE** and is **out of scope** for Pixel1HLE.

## Source → target mapping

| Reference | Responsibility | iOS-specific | Pixel1HLE equivalent | Action | Tests |
|-----------|----------------|--------------|----------------------|--------|-------|
| `cpu.rs` + dynarmic_wrapper | DBT execution | ARM32 | `cpu::elf_loader::AArch64Backend` | **Reimplement** AArch64 backend later | Backend trait tests |
| `mem.rs` | Guest memory | No | Future `guest_mem` | Adapt pattern | R/W bounds tests |
| `abi.rs` | Guest calls | ARM32 ABI | Future AAPCS64 | Reimplement | Call stub tests |
| `mach_o.rs` / `dyld.rs` | Binary load + link | Mach-O/dyld | `cpu/elf_loader.rs`, `cpu/linker.rs` | **Reimplement ELF** | PT_LOAD + RELA unit tests |
| `gles.rs` | GLES HLE | Layer kits | `hardware/gles_bridge.rs` | Adapt ideas | After execution |
| `fs.rs` / `bundle.rs` | Package FS | Bundle | `apk_inspect`, `platform/paths` | Reimplement for APK | APK inspect tests |
| `android_jni.rs` | JNI **host** glue for Android UI | Host-side | Not guest JNI | Exclude / separate | — |
| `objc/*`, `frameworks/*` | Apple frameworks | Yes | None | **Exclude** | — |
| `environment.rs` | Threads / app state | Mixed | `ui` + future runtime | Adapt lifecycle only | — |

## Android guest environment (Pixel1HLE)

1. **APK** — ZIP + native libs (`apk_inspect`). Binary AXML package name still TODO.
2. **ELF** — PT_LOAD + RELA AArch64 (`elf_loader`).
3. **CPU** — **Required** for Windows x86-64: AArch64 DBT/interpreter (Dynarmic A64, Unicorn, etc.). Default backend is `NullAArch64Backend` (honest failure).
4. **JNI / ART** — **Out of scope** until guest code runs. Native-only path first (Unity/AIR `.so`).
5. **Java/Kotlin bytecode** — **Not supported** in current architecture.

## What was implemented in this milestone

- `src/cpu/elf_loader.rs`: PT_LOAD mapping, RELA types `RELATIVE` / `ABS64` / `GLOB_DAT` / `JUMP_SLOT`, bounds-checked image R/W, `AArch64Backend` + `NullAArch64Backend`.
- Unit tests: reject garbage, map A64 ELF text, null backend refuses `run`, VA bounds, u64 R/W.

## Commands (run on a current Rust toolchain)

```bash
cargo test --features desktop,loader -- elf_loader
cargo test --features desktop,loader -- apk_inspect
cargo check --features desktop,loader
```

Observed in the audit environment: Cargo 1.75 could not resolve some crates (edition2024). Use Rust **1.78+** locally/CI.
