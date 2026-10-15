//! Android Storage Access Framework (SAF) – open system file manager for APKs
//!
//! Launches `Intent.ACTION_OPEN_DOCUMENT` filtered to
//! `application/vnd.android.package-archive`. Full `onActivityResult` delivery
//! requires the host Activity; after the intent is fired we also watch the
//! sandboxed `apps/` directory for newly copied packages.

use crate::error::{Pixel1Error, Result};
use crate::platform::paths::{apps_dir, ensure_sandbox_dirs};
use log::{info, warn};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Tracks a pending SAF / import operation so the UI can poll for new APKs.
#[derive(Debug, Clone)]
pub struct PendingSafImport {
    pub started: Instant,
    pub apps_snapshot: Vec<(String, u64)>,
}

impl PendingSafImport {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            apps_snapshot: list_apps_apk_sizes(),
        }
    }

    /// Returns a newly appeared APK path once the listing stabilizes.
    pub fn poll_new_apk(&self) -> Option<PathBuf> {
        if self.started.elapsed() < Duration::from_millis(400) {
            return None;
        }
        let now = list_apps_apk_sizes();
        for (name, size) in &now {
            let was = self.apps_snapshot.iter().find(|(n, _)| n == name);
            match was {
                None if *size > 0 => {
                    return Some(apps_dir().join(name));
                }
                Some((_, old)) if size > old && *size > 0 => {
                    // still growing – wait
                }
                Some((_, old)) if size == old && *size > 0 => {
                    // settled new or updated file
                    if was.map(|(_, o)| o != size).unwrap_or(true) {
                        return Some(apps_dir().join(name));
                    }
                }
                _ => {}
            }
        }
        // Brand-new names only
        for (name, size) in &now {
            if *size > 0 && !self.apps_snapshot.iter().any(|(n, _)| n == name) {
                return Some(apps_dir().join(name));
            }
        }
        None
    }
}

fn list_apps_apk_sizes() -> Vec<(String, u64)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(apps_dir()) {
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) == Some("apk") {
                let name = e.file_name().to_string_lossy().into_owned();
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((name, size));
            }
        }
    }
    out.sort();
    out
}

/// Fire the system document picker for APK MIME type via JNI.
pub fn launch_apk_document_picker() -> Result<()> {
    ensure_sandbox_dirs().map_err(|e| Pixel1Error::Saf(e.to_string()))?;

    info!(target: "hle::saf", "Launching Android SAF ACTION_OPEN_DOCUMENT for APK");

    // JNI path – best-effort; failures fall through to logged guidance
    match launch_open_document_jni() {
        Ok(()) => {
            info!(target: "hle::saf", "SAF intent dispatched – pick an APK, then save/copy into apps/");
            Ok(())
        }
        Err(e) => {
            warn!(target: "hle::saf", "SAF JNI launch failed: {e}");
            warn!(
                target: "hle::saf",
                "Fallback: place .apk in {} or use the in-app explorer",
                apps_dir().display()
            );
            Err(e)
        }
    }
}

fn launch_open_document_jni() -> Result<()> {
    // ndk-context provides the current Android context when running under NativeActivity
    let ctx = ndk_context::android_context();
    let vm = ctx.vm() as *mut jni::sys::JavaVM;
    if vm.is_null() {
        return Err(Pixel1Error::Saf("JavaVM is null".into()));
    }
    let activity = ctx.context() as jni::sys::jobject;
    if activity.is_null() {
        return Err(Pixel1Error::Saf("Activity context is null".into()));
    }

    // SAFETY: JNI env attach for the duration of this call
    let java_vm = unsafe { jni::JavaVM::from_raw(vm) }
        .map_err(|e| Pixel1Error::Saf(format!("JavaVM::from_raw: {e}")))?;
    let mut env = java_vm
        .attach_current_thread()
        .map_err(|e| Pixel1Error::Saf(format!("attach: {e}")))?;

    // Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
    let intent_class = env
        .find_class("android/content/Intent")
        .map_err(|e| Pixel1Error::Saf(format!("Intent class: {e}")))?;
    let action = env
        .new_string("android.intent.action.OPEN_DOCUMENT")
        .map_err(|e| Pixel1Error::Saf(format!("action string: {e}")))?;
    let intent = env
        .new_object(
            &intent_class,
            "(Ljava/lang/String;)V",
            &[(&action).into()],
        )
        .map_err(|e| Pixel1Error::Saf(format!("new Intent: {e}")))?;

    // intent.addCategory(Intent.CATEGORY_OPENABLE);
    let category = env
        .new_string("android.intent.category.OPENABLE")
        .map_err(|e| Pixel1Error::Saf(format!("category: {e}")))?;
    env.call_method(
        &intent,
        "addCategory",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[(&category).into()],
    )
    .map_err(|e| Pixel1Error::Saf(format!("addCategory: {e}")))?;

    // intent.setType("application/vnd.android.package-archive");
    let mime = env
        .new_string("application/vnd.android.package-archive")
        .map_err(|e| Pixel1Error::Saf(format!("mime: {e}")))?;
    env.call_method(
        &intent,
        "setType",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[(&mime).into()],
    )
    .map_err(|e| Pixel1Error::Saf(format!("setType: {e}")))?;

    // Also accept */* as some OEMs mis-label APKs
    let extra_mime_key = env
        .new_string("android.intent.extra.MIME_TYPES")
        .map_err(|e| Pixel1Error::Saf(e.to_string()))?;
    let mime2 = env
        .new_string("*/*")
        .map_err(|e| Pixel1Error::Saf(e.to_string()))?;
    let mime_array = env
        .new_object_array(2, "java/lang/String", &mime)
        .map_err(|e| Pixel1Error::Saf(e.to_string()))?;
    env.set_object_array_element(&mime_array, 0, &mime)
        .map_err(|e| Pixel1Error::Saf(e.to_string()))?;
    env.set_object_array_element(&mime_array, 1, &mime2)
        .map_err(|e| Pixel1Error::Saf(e.to_string()))?;
    let _ = env.call_method(
        &intent,
        "putExtra",
        "(Ljava/lang/String;[Ljava/lang/String;)Landroid/content/Intent;",
        &[(&extra_mime_key).into(), (&mime_array).into()],
    );

    // activity.startActivity(intent);
    let activity_obj = unsafe { jni::objects::JObject::from_raw(activity) };
    env.call_method(
        &activity_obj,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[(&intent).into()],
    )
    .map_err(|e| Pixel1Error::Saf(format!("startActivity: {e}")))?;

    // Prevent double-free of the local activity ref we didn't own
    std::mem::forget(activity_obj);

    Ok(())
}

/// Copy a host-accessible file into the sandboxed apps/ tree (plain name only).
pub fn import_apk_to_sandbox(src: &std::path::Path) -> Result<PathBuf> {
    use crate::platform::paths::{apps_dest_for, is_plain_name};

    ensure_sandbox_dirs().map_err(|e| Pixel1Error::Import(e.to_string()))?;

    let file_name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Pixel1Error::Import("invalid file name".into()))?;

    if !is_plain_name(file_name) {
        return Err(Pixel1Error::Import(format!(
            "refusing unsafe name: {file_name}"
        )));
    }

    let dest = apps_dest_for(file_name)
        .ok_or_else(|| Pixel1Error::Import("not a plain .apk name".into()))?;

    std::fs::copy(src, &dest).map_err(|e| Pixel1Error::Import(format!("copy: {e}")))?;
    info!(target: "hle::saf", "Imported {} → {}", src.display(), dest.display());
    Ok(dest)
}
