//! Cross-command state shared via Tauri's `State<>` injector.
//!
//! Currently holds only `ProjectPathsCache` — the resolved path layout
//! for the currently open project. Future shared state (caches,
//! metrics, in-memory indexes, etc.) goes here.

use std::sync::{Arc, RwLock};

use crate::path_config::Paths;

/// Resolved path layout for the currently open project.
///
/// - Set by `commands::project::open_project` after it has loaded +
///   validated the project-level + global yaml.
/// - Set by `commands::project::create_project` after the new
///   project's layout is finalized.
/// - Read by every command that needs the layout (file_sync, fs,
///   extract_images, vectorstore, etc.) via `tauri::State` injection,
///   rather than re-resolving on every call.
/// - Cleared by `commands::project::close_project` (when implemented
///   in a future task) or whenever the active project changes.
///
/// **Why cache it instead of re-resolving every time?** Path
/// resolution itself is cheap (two yaml reads + a struct clone), but
/// caching gives every command a *consistent snapshot* of the layout
/// for the duration of an operation. Without the cache, if the
/// user edits the yaml mid-operation, two commands reading the
/// same data could see different layouts.
///
/// `RwLock<Option<Paths>>` so reads (the common case — every
/// command reads) don't block each other; writes only block briefly
/// during open/create. Wrapped in `Arc` so cloning the cache struct
/// (e.g. into a background task) is cheap.
#[derive(Default, Clone)]
pub struct ProjectPathsCache {
    inner: Arc<RwLock<Option<Paths>>>,
}

impl ProjectPathsCache {
    /// Construct an empty cache. Use `Self::default()` or this — both
    /// produce the same `Option::None` starting state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a clone of the cached layout, or `None` if no project
    /// is currently open.
    pub fn get(&self) -> Option<Paths> {
        self.inner.read().ok().and_then(|g| g.clone())
    }

    /// Overwrites the cached layout. Used by `open_project` and
    /// `create_project`. No-op if the inner lock is poisoned (which
    /// would only happen if a writer panicked mid-update).
    pub fn set(&self, paths: Paths) {
        if let Ok(mut g) = self.inner.write() {
            *g = Some(paths);
        }
    }

    /// Drops the cached layout. Used by project-close / project-switch
    /// flows. No-op if the inner lock is poisoned.
    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.write() {
            *g = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cache_returns_none() {
        let cache = ProjectPathsCache::new();
        assert!(cache.get().is_none());
    }

    #[test]
    fn set_then_get_returns_same_layout() {
        let cache = ProjectPathsCache::new();
        let mut paths = Paths::default();
        paths.raw_sources = PathBuf::from("docs/inbox");
        cache.set(paths.clone());
        let got = cache.get().unwrap();
        assert_eq!(got.raw_sources, PathBuf::from("docs/inbox"));
    }

    #[test]
    fn clear_drops_layout() {
        let cache = ProjectPathsCache::new();
        cache.set(Paths::default());
        assert!(cache.get().is_some());
        cache.clear();
        assert!(cache.get().is_none());
    }

    #[test]
    fn clone_shares_state() {
        // Cloning the cache (e.g. into a Tauri command thread) MUST
        // share the same underlying lock — otherwise the State<>
        // injector would give each command its own private cache and
        // nothing would actually be cached.
        let a = ProjectPathsCache::new();
        let b = a.clone();
        a.set(Paths::default());
        assert!(b.get().is_some(), "cloned cache must observe the original's writes");
    }

    use std::path::PathBuf;
}
