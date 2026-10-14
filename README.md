# pixel1-translator

**High-Level Emulation (HLE) compatibility layer** for Android 7.1 apps compiled for the Google Pixel 1 (`sailfish`).

Now includes a full **OpenGL ES → Desktop GL** translation bridge aimed at Unity (`libunity.so`) and similar native engines.

## Architecture

```
Android App / libunity.so (ARM64)
         │
         ▼
┌─────────────────────────────────────────────┐
│  BinderRuntime                              │
└────────────────────┬────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────┐
│  HardwareStack + GLES Bridge                │
│  extern "C" egl* / gl*  →  glow::Context    │
│  (glutin Core GL 4.5, 1920×1080@60)         │
└────────────────────┬────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────┐
│  SyscallTranslator                          │
└────────────────────┬────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────┐
│  CpuRuntime (ELF + JNI + Unity bootstrap)   │
└─────────────────────────────────────────────┘
```

## GLES / EGL symbols exported

`eglGetDisplay`, `eglInitialize`, `eglChooseConfig`, `eglCreateWindowSurface`,
`eglCreateContext`, `eglMakeCurrent`, `eglSwapBuffers`, `eglGetError`, …

`glViewport`, `glClear`, `glClearColor`, `glGenBuffers`, `glBindBuffer`,
`glBufferData`, `glDrawElements`, `glEnable`, `glDisable`, `glGetString`, …

All of them are real `extern "C"` entry points that forward into a single global `glow::Context`.

## Quick start

```bash
cargo run --release
```

You will see the full Unity-style cascade:

```
eglGetDisplay → fake display
eglInitialize
eglChooseConfig
eglCreateWindowSurface → 1920x1080 sailfish surface
eglMakeCurrent
glViewport(0, 0, 1920, 1080)
glClearColor(...)
glClear(...)
glGenBuffers / glBindBuffer / glBufferData
```

## Profile

Everything is driven by `config/pixel1-sailfish.yaml`.

## Status

- ✅ GLES/EGL HLE bridge (no todo!() in render path)
- ✅ Global glow context reachable from guest code
- ✅ Unity bootstrap demo that exercises the whole pipeline
- Next: real frame loop + swap, texture upload path, more GLES 3.x entry points
