//! The tree seal: one SHA-256 over every file and link of a directory tree. A person-owned
//! install is trusted only while its tree still has the seal the manifest pins for it, so the
//! same walk and the same lines are written by `tools/seal.py`, which pinned them: change this
//! text and every pinned seal changes with it.
//!
//! The text hashed is one line per file or link, sorted by the path (as bytes), each ending in a
//! newline: `F <x or -> <size> <sha256 of the content> <path>` for a file (`x` when any execute
//! bit is set), `L <target> <path>` for a symbolic link. Paths are relative to the root and use
//! `/`. Directories are not listed. The owner and the exact mode bits are not part of it, so
//! an extraction by any user and any umask seals the same.
//!
//! This module is the lines and the hash. Walking a tree to make them is `engine-probe`'s.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// A tree's seal as 64 lower-case hex digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeSeal(String);

impl TreeSeal {
    /// The seal of a tree whose entries made these lines.
    pub fn of(mut lines: Vec<SealLine>) -> Self {
        lines.sort();
        let mut hasher = Sha256::new();
        for line in &lines {
            hasher.update(line.text.as_bytes());
        }
        Self(hex(&hasher.finalize()))
    }

    /// The hex text.
    pub fn as_hex(&self) -> &str {
        &self.0
    }

    /// Whether this seal is the pinned `hex`.
    pub fn is(&self, hex: &str) -> bool {
        self.0 == hex
    }
}

/// One entry of a tree as the line it makes and the path that sorts it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SealLine {
    path: String,
    text: String,
}

impl SealLine {
    /// A file at `rel`: whether any execute bit is set, its size and the SHA-256 of its content.
    pub fn file(rel: &str, executable: bool, size: u64, digest: &str) -> Self {
        let mode = if executable { 'x' } else { '-' };
        Self {
            path: rel.to_owned(),
            text: format!("F {mode} {size} {digest} {rel}\n"),
        }
    }

    /// A symbolic link at `rel` pointing at `target`.
    pub fn link(rel: &str, target: &str) -> Self {
        Self {
            path: rel.to_owned(),
            text: format!("L {target} {rel}\n"),
        }
    }
}

/// Lower-case hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _written = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The digest `tools/seal.py`, which pins the manifest's seals, gives the tree
    /// `a` ("hello\n"), `d/run` (executable "#!/bin/sh\n") and `d/link -> run`: the two must
    /// agree. `engine-probe` makes the same seal from a real tree.
    #[test]
    fn lines_have_the_seal_the_pinning_tool_gives_them() {
        let lines = vec![
            SealLine::link("d/link", "run"),
            SealLine::file(
                "d/run",
                true,
                10,
                "a8076d3d28d21e02012b20eaf7dbf75409a6277134439025f282e368e3305abf",
            ),
            SealLine::file(
                "a",
                false,
                6,
                "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03",
            ),
        ];
        assert_eq!(
            TreeSeal::of(lines).as_hex(),
            "ab8e8a27eed121708fa02d6eb4112de90120f73122f6e5f77071ed54cb5952e3"
        );
    }

    #[test]
    fn the_order_the_lines_come_in_does_not_matter_and_a_changed_line_does() {
        let a = SealLine::link("a", "x");
        let b = SealLine::file("b", false, 0, "00");
        let both = TreeSeal::of(vec![a.clone(), b.clone()]);
        assert_eq!(both, TreeSeal::of(vec![b.clone(), a.clone()]));
        assert_ne!(
            both,
            TreeSeal::of(vec![a, SealLine::file("b", true, 0, "00")])
        );
        assert!(both.is(both.as_hex()));
        assert_eq!(hex(&[0x0a, 0xff]), "0aff");
    }
}
