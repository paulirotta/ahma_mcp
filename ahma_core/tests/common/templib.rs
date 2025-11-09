use std::io::Result;
use tempfile::TempDir;

/// Lightweight wrapper around tempfile::TempDir to match project conventions.
pub fn tempdir() -> Result<TempDir> {
    TempDir::new()
}

pub type Directory = TempDir;
