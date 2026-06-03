//! Tauri commands for the `paths.yaml` settings panel.
//!
//! These three commands are the IPC surface the frontend
//! `PathConfigPanel` (Task 11) calls to read, write, and reset a
//! project's per-project `paths.yaml` override. They do NOT need
//! `AppHandle` — they take the project path as a string and read /
//! write the file directly. The cache is updated the next time the
//! user re-opens the project (which is the natural reload point —
//! the user just saved a config, they should see it take effect).
//!
//! If we wanted to update the cache without a re-open, we'd need to
//! add `tauri::State<'_, ProjectPathsCache>` to the signatures and
//! call `cache.set(resolved)` after re-resolving. The plan defers
//! that to a follow-up (the UI shows a "Reopen project to apply"
//! hint instead).

use std::path::Path;

use crate::path_config::{self, PathsFile};

/// Get the current project's `path_config`.
///
/// Returns the on-disk `PathsFile` if `.llm-wiki/paths.yaml` exists,
/// or a synthesized `PathsFile` with `Paths::default()` and the
/// current schema version (i.e. what the user would see after a
/// "Reset") if not. Either way the caller gets a well-formed
/// `PathsFile` they can show in the settings panel.
#[tauri::command]
pub fn get_path_config(project_path: String) -> Result<PathsFile, String> {
    let root = Path::new(&project_path);
    match path_config::load_project_yaml(root) {
        Ok(Some(file)) => Ok(file),
        Ok(None) => Ok(PathsFile {
            version: path_config::CURRENT_SCHEMA_VERSION,
            paths: path_config::Paths::default(),
        }),
        Err(e) => Err(format!("Failed to read paths.yaml: {e}")),
    }
}

/// Save a new `PathsFile` to the project's `.llm-wiki/paths.yaml`.
/// Atomic write (via `save_project_yaml`'s tmp + rename); safe to
/// call while a project is open. The cache is NOT updated — the
/// user must re-open the project to see the new layout take
/// effect (the UI surfaces this with a "Reopen to apply" hint).
#[tauri::command]
pub fn set_path_config(project_path: String, file: PathsFile) -> Result<(), String> {
    let root = Path::new(&project_path);
    path_config::save_project_yaml(root, &file)
        .map_err(|e| format!("Failed to write paths.yaml: {e}"))
}

/// Delete the project's `.llm-wiki/paths.yaml`, reverting to the
/// built-in default layout. No-op (and not an error) if the file
/// doesn't exist. Like `set_path_config`, the cache is not updated
/// — the user re-opens the project to see defaults take effect.
#[tauri::command]
pub fn reset_path_config(project_path: String) -> Result<(), String> {
    let root = Path::new(&project_path);
    let path = root.join(".llm-wiki/paths.yaml");
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete paths.yaml: {e}"))?;
    }
    Ok(())
}
