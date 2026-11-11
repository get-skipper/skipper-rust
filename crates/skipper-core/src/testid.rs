use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Canonical form used for every lookup: trim, lowercase, collapse whitespace.
pub fn normalize_test_id(id: &str) -> String {
    id.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// `relative/path.rs > part1 > part2`. The path is made relative to the
/// project root (see [`find_project_root`]) and always uses `/`.
pub fn build_test_id<S: AsRef<str>>(file: &str, parts: &[S]) -> String {
    let mut id = relative_to_root(file);
    for p in parts {
        id.push_str(" > ");
        id.push_str(p.as_ref());
    }
    id
}

/// Make `file` relative to the project root. Relative paths (what `file!()`
/// yields for workspace members) are kept as they are.
pub fn relative_to_root(file: &str) -> String {
    let path = Path::new(file);
    let rel = if path.is_absolute() {
        path.strip_prefix(project_root()).unwrap_or(path)
    } else {
        path
    };
    rel.to_string_lossy().replace('\\', "/")
}

fn project_root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let start = std::env::var_os("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        find_project_root(&start)
    })
}

/// Outermost ancestor whose `Cargo.toml` declares `[workspace]`, else the
/// nearest directory with a `Cargo.toml`, else `start` itself.
pub fn find_project_root(start: &Path) -> PathBuf {
    let mut nearest = None;
    let mut workspace = None;
    for dir in start.ancestors() {
        let Ok(manifest) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        nearest.get_or_insert_with(|| dir.to_path_buf());
        if manifest.lines().any(|l| l.trim() == "[workspace]") {
            workspace = Some(dir.to_path_buf());
        }
    }
    workspace.or(nearest).unwrap_or_else(|| start.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(
            normalize_test_id("  Tests/Auth.rs  >\tLogin   >  Works "),
            "tests/auth.rs > login > works"
        );
    }

    #[test]
    fn builds_with_forward_slashes() {
        assert_eq!(
            build_test_id("tests\\auth.rs", &["mod_a", "it works"]),
            "tests/auth.rs > mod_a > it works"
        );
    }

    #[test]
    fn finds_outermost_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        let member = ws.join("crates/a");
        std::fs::create_dir_all(member.join("src")).unwrap();
        std::fs::write(ws.join("Cargo.toml"), "[workspace]\nmembers=[]\n").unwrap();
        std::fs::write(member.join("Cargo.toml"), "[package]\nname=\"a\"\n").unwrap();
        assert_eq!(find_project_root(&member.join("src")), ws);

        std::fs::write(ws.join("Cargo.toml"), "# not a workspace\n").unwrap();
        assert_eq!(find_project_root(&member.join("src")), member);
    }
}
