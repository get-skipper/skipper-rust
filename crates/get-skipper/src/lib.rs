//! Skipper for Rust: enable or disable tests from a Google Spreadsheet.
//!
//! ```ignore
//! use get_skipper::skipper;
//!
//! #[skipper]
//! #[test]
//! fn can_log_in() { /* … */ }
//! ```
//!
//! Configure with environment variables (`SKIPPER_SPREADSHEET_ID`,
//! `GOOGLE_CREDS_B64` or `SKIPPER_CREDENTIALS_FILE`, `SKIPPER_SHEET_NAME`),
//! e.g. in `.cargo/config.toml` under `[env]`. Without them every test runs.
//! Run through `cargo skipper test` (crate `cargo-skipper`) to fetch the sheet
//! once per run, get the quarantine report and use `SKIPPER_MODE=sync`.

use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

pub use skipper_core::*;
pub use skipper_macros::skipper;

/// Skip the current test if it is disabled. For code where the `#[skipper]`
/// attribute cannot go; must be called directly in the test function body.
/// Expands to an early `return` (use `skip_if_disabled!(Ok(()))` in tests
/// returning `Result`).
#[macro_export]
macro_rules! skip_if_disabled {
    () => {
        $crate::skip_if_disabled!(())
    };
    ($ret:expr) => {{
        fn __skipper_here() {}
        let name = $crate::__fn_name(::core::any::type_name_of_val(&__skipper_here));
        if $crate::__should_skip(::core::file!(), ::core::module_path!(), name) {
            return $ret;
        }
    }};
}

#[doc(hidden)]
pub fn __fn_name(type_name: &str) -> &str {
    // "krate::module::test_fn::__skipper_here" (maybe with "::{{closure}}" for async).
    type_name
        .split("::")
        .filter(|s| *s != "__skipper_here" && !s.starts_with("{{"))
        .last()
        .unwrap_or(type_name)
}

fn resolver() -> Option<&'static SkipperResolver> {
    static RESOLVER: OnceLock<Option<SkipperResolver>> = OnceLock::new();
    RESOLVER
        .get_or_init(|| match SkipperResolver::from_env() {
            Ok(r) => r,
            // Only reached with SKIPPER_FAIL_OPEN=false: make the test fail loudly.
            Err(e) => panic!("[skipper] cannot load spreadsheet: {e}"),
        })
        .as_ref()
}

/// Test ID used by `#[skipper]`: `file > module::path > fn` (crate name dropped).
pub fn test_id(file: &str, module_path: &str, name: &str) -> String {
    let modules = module_path.split_once("::").map(|(_, rest)| rest);
    match modules {
        Some(m) if !m.is_empty() => build_test_id(file, &[m, name]),
        _ => build_test_id(file, &[name]),
    }
}

#[doc(hidden)]
pub fn __should_skip(file: &str, module_path: &str, name: &str) -> bool {
    let Some(resolver) = resolver() else {
        return false;
    };
    let id = test_id(file, module_path, name);
    if let Some(dir) = std::env::var_os("SKIPPER_DISCOVERED_DIR") {
        if let Err(e) =
            CacheManager::write_discovered_ids(Path::new(&dir), std::slice::from_ref(&id))
        {
            warn(format!("cannot record discovered test {id}: {e}"));
        }
    }
    if resolver.is_test_enabled(&id) {
        return false;
    }
    // Straight to the stderr handle: libtest captures eprintln! of passing tests.
    let _ = writeln!(std::io::stderr(), "{}", resolver.skip_message(&id));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_drops_crate_name() {
        assert_eq!(
            test_id("tests/auth.rs", "auth", "login"),
            "tests/auth.rs > login"
        );
        assert_eq!(
            test_id("src/lib.rs", "my_crate::a::tests", "works"),
            "src/lib.rs > a::tests > works"
        );
    }

    #[test]
    fn fn_name_from_type_name() {
        assert_eq!(__fn_name("k::m::my_test::__skipper_here"), "my_test");
        assert_eq!(
            __fn_name("k::my_test::{{closure}}::__skipper_here"),
            "my_test"
        );
    }
}
