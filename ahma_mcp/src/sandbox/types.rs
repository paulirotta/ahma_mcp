use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    Strict,
    Test,
}

/// A guard that holds a read lock on the sandbox scopes.
pub struct ScopesGuard<'a>(pub(super) std::sync::RwLockReadGuard<'a, Vec<PathBuf>>);

impl std::ops::Deref for ScopesGuard<'_> {
    type Target = [PathBuf];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Debug for ScopesGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
