use thiserror::Error;

#[derive(Error, Debug)]
pub enum Pixel1Error {
    #[error("ELF load failed: {0}")]
    ElfLoad(String),

    #[error("Memory mapping error: {0}")]
    MemMap(#[from] std::io::Error),

    #[error("Binder transaction failed: {0}")]
    Binder(String),

    #[error("Service denied by policy (deny-by-default): {0}")]
    ServiceDenied(String),

    #[error("Hardware feature absent in sailfish profile: {0}")]
    FeatureAbsent(String),

    #[error("Syscall translation failed: {0}")]
    Syscall(String),

    #[error("JNI hook table error: {0}")]
    JniHook(String),

    #[error("Config / profile error: {0}")]
    Config(String),

    #[error("Display / window error: {0}")]
    Display(String),

    #[error("GLES / EGL error: {0}")]
    Gles(String),

    #[error("Audio pipeline error: {0}")]
    Audio(String),

    #[error("Storage router error: {0}")]
    Storage(String),

    #[error("Import / file picker error: {0}")]
    Import(String),

    #[error("Android SAF / storage permission error: {0}")]
    Saf(String),
}

pub type Result<T> = std::result::Result<T, Pixel1Error>;
