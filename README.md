# pixel1-translator

**High-Level Emulation (HLE) compatibility layer** for Android 7.1 apps compiled for the Google Pixel 1 (`sailfish`).

Runs ARM64 native code + Android framework calls **natively on the host** without a full VM / hypervisor.

## Architecture

```
Android App (ARM64 .so + ART/Java)
         │
         ▼
┌─────────────────────────────────────────────┐
│  BinderRuntime                              │
│  - getSystemService() trap                  │
│  - deny-by-default + manifest check         │
│  - lazy service spin-up                     │
└────────────────────┬────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────┐
│  HardwareStack (profile-driven)             │
│  - Display 1920×1080 @ 60 Hz                │
│  - Multi-touch injection                    │
│  - Audio pipeline                           │
│  - Sparse 32 GB storage                     │
│  - Battery permanently 100 % + charging     │
│  - hasSystemFeature() → false for absent HW │
└────────────────────┬────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────┐
│  SyscallTranslator (Linux 3.18 → host)      │
│  - epoll / futex / mmap / openat …          │
└────────────────────┬────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────┐
│  CpuRuntime                                 │
│  - ELF loader (object + memmap2)            │
│  - Guest → host address mapping             │
│  - JNI native method hook tables            │
└─────────────────────────────────────────────┘
```

## Quick start

```bash
cargo run --release
```

## Profile

Everything is driven by `config/pixel1-sailfish.yaml`.  
If a component is marked `present: false`, any `hasSystemFeature()` or `getSystemService()` for it returns `false` / `null` instantly.

## Status

Architectural skeleton + full cascade demo.  
Next milestones: real winit/glutin window, cpal audio, proper Binder parcel marshalling, bionic linker emulation.
