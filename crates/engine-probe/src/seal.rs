//! Sealing a tree on disk: the walk that makes the lines `engine-manifest`'s [`TreeSeal`] hashes.

use engine_manifest::{SealLine, TreeSeal, hex};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Why a tree could not be sealed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SealError {
    /// A file or directory could not be read.
    #[error("cannot read {path}: {kind}")]
    Io {
        /// What was being read.
        path: PathBuf,
        /// What the system said.
        kind: std::io::ErrorKind,
    },
    /// Something that is not a file, a directory or a link (a socket, a device).
    #[error("{0} is not a file, a directory or a link")]
    Unsupported(PathBuf),
    /// A name or a link target that is not text.
    #[error("{0} has a name that is not text")]
    NotText(PathBuf),
}

fn io(path: &Path, error: &std::io::Error) -> SealError {
    SealError::Io {
        path: path.to_path_buf(),
        kind: error.kind(),
    }
}

/// The size and SHA-256 of a file's content.
///
/// # Errors
/// [`SealError::Io`] when the file cannot be read.
pub fn file_digest(path: &Path) -> Result<(u64, String), SealError> {
    let mut file = std::fs::File::open(path).map_err(|e| io(path, &e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(|e| io(path, &e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((size, hex(&hasher.finalize())))
}

/// The line `path` makes, or `None` for a directory.
fn line_of(root: &Path, path: &Path) -> Result<Option<SealLine>, SealError> {
    let rel = path
        .strip_prefix(root)
        .ok()
        .and_then(Path::to_str)
        .ok_or_else(|| SealError::NotText(path.to_path_buf()))?;
    let meta = std::fs::symlink_metadata(path).map_err(|e| io(path, &e))?;
    let kind = meta.file_type();
    if kind.is_dir() {
        return Ok(None);
    }
    if kind.is_symlink() {
        let target = std::fs::read_link(path).map_err(|e| io(path, &e))?;
        let target = target
            .to_str()
            .ok_or_else(|| SealError::NotText(path.to_path_buf()))?;
        Ok(Some(SealLine::link(rel, target)))
    } else if kind.is_file() {
        let executable = meta.permissions().mode() & 0o111 != 0;
        let (size, digest) = file_digest(path)?;
        Ok(Some(SealLine::file(rel, executable, size, &digest)))
    } else {
        Err(SealError::Unsupported(path.to_path_buf()))
    }
}

/// The seal of the tree at `root`.
///
/// # Errors
/// See [`SealError`].
pub fn seal_of(root: &Path) -> Result<TreeSeal, SealError> {
    let mut lines = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| io(&dir, &e))? {
            let path = entry.map_err(|e| io(&dir, &e))?.path();
            if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_dir()) {
                pending.push(path.clone());
            }
            lines.extend(line_of(root, &path)?);
        }
    }
    Ok(TreeSeal::of(lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(root: &Path, mode: u32, name: &str, body: &str) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(&path, body).expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("mode");
    }

    fn sample(dir: &tempfile::TempDir) -> PathBuf {
        let root = dir.path().join("tree");
        make(&root, 0o644, "a", "hello\n");
        make(&root, 0o755, "d/run", "#!/bin/sh\n");
        std::os::unix::fs::symlink("run", root.join("d/link")).expect("link");
        root
    }

    /// The digest `tools/seal.py`, which pins the manifest's seals, gives the same tree: the two
    /// must agree.
    #[test]
    fn a_tree_has_the_seal_the_pinning_tool_gives_it() {
        let dir = tempfile::tempdir().expect("scratch dir");
        let root = sample(&dir);
        assert_eq!(
            seal_of(&root).expect("seal").as_hex(),
            "ab8e8a27eed121708fa02d6eb4112de90120f73122f6e5f77071ed54cb5952e3"
        );
    }

    #[test]
    fn a_changed_added_or_removed_entry_changes_the_seal_and_an_exact_mode_does_not() {
        let dir = tempfile::tempdir().expect("scratch dir");
        let root = sample(&dir);
        let before = seal_of(&root).expect("seal");
        std::fs::set_permissions(root.join("a"), std::fs::Permissions::from_mode(0o600))
            .expect("mode");
        std::fs::set_permissions(root.join("d/run"), std::fs::Permissions::from_mode(0o700))
            .expect("mode");
        assert_eq!(
            seal_of(&root).expect("seal"),
            before,
            "the exact mode is not sealed"
        );
        std::fs::set_permissions(root.join("d/run"), std::fs::Permissions::from_mode(0o644))
            .expect("mode");
        assert_ne!(seal_of(&root).expect("seal"), before, "the execute bit is");
        std::fs::set_permissions(root.join("d/run"), std::fs::Permissions::from_mode(0o755))
            .expect("mode");
        make(&root, 0o644, "a", "hello!\n");
        assert_ne!(seal_of(&root).expect("seal"), before, "a changed file");
        make(&root, 0o644, "a", "hello\n");
        assert_eq!(seal_of(&root).expect("seal"), before);
        make(&root, 0o644, "extra", "");
        assert_ne!(seal_of(&root).expect("seal"), before, "an added file");
        std::fs::remove_file(root.join("extra")).expect("remove");
        std::fs::remove_file(root.join("d/link")).expect("remove");
        assert_ne!(seal_of(&root).expect("seal"), before, "a removed link");
    }

    #[test]
    fn a_tree_that_is_not_there_is_an_error_not_a_seal() {
        let dir = tempfile::tempdir().expect("scratch dir");
        assert!(matches!(
            seal_of(&dir.path().join("gone")),
            Err(SealError::Io { .. })
        ));
    }
}
