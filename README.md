# pixel1-translator

**High-Level Emulation** for Android 7.1 (Pixel 1 / sailfish) with full **UI lifecycle**, dynamic linker and GLES bridge.

## UI State Machine

```
AppPicker  →  BootAnimation  →  ApkLoading  →  GameRunning
   │               │                 │               │
 egui drawer   spinning logo   progress bar    pure GLES
```

- **AppPicker**: vintage Android drawer, scans `./` and `./apks/` for `.apk` files
- **BootAnimation**: pure-egui pulsing/spinning Android-style logo (~2.2 s)
- **ApkLoading**: live progress fed by the HLE linker (extract → map → relocate → JNI_OnLoad)
- **GameRunning**: UI drops, `libunity.so` owns the 1920×1080 canvas

## Quick start

```bash
cargo run --release
```

Drop any `.apk` that contains `lib/arm64-v8a/*.so` next to the binary (or inside `apks/`) and click it in the picker.

## Architecture

```
APK
 → ApkElfExecutor (progress → UI)
 → PT_LOAD + RELA + JNI_OnLoad
 → GLES Bridge (egl*/gl* → glow)
 → 1920×1080 sailfish surface
```

## Status

| Component              | Status |
|------------------------|--------|
| egui state machine     | ✅     |
| Boot animation         | ✅     |
| Live loading progress  | ✅     |
| APK picker             | ✅     |
| Dynamic linker         | ✅     |
| GLES HLE bridge        | ✅     |
