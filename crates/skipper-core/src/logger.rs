/// True when `SKIPPER_DEBUG` is set to a non-empty value.
pub fn debug_enabled() -> bool {
    std::env::var("SKIPPER_DEBUG").is_ok_and(|v| !v.is_empty())
}

/// Debug log to stderr, only when `SKIPPER_DEBUG` is set.
pub fn log(msg: impl AsRef<str>) {
    if debug_enabled() {
        eprintln!("[skipper] {}", msg.as_ref());
    }
}

/// Warning to stderr, only when `SKIPPER_DEBUG` is set.
pub fn warn(msg: impl AsRef<str>) {
    if debug_enabled() {
        eprintln!("[skipper] WARN: {}", msg.as_ref());
    }
}
