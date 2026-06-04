//! Project path configuration: parse, resolve, validate.
//!
//! See `docs/plans/2026-06-01-configurable-paths.md` for the design.
//!
//! Two file levels are supported:
//!   1. **Project-level** — `<project_root>/paths.yaml` (per-project override)
//!   2. **Global**       — `<app_data_dir>/paths.yaml` (cross-project default)
//!
//! Resolution order for each path key (see `resolve_paths` in Task 3):
//!   project  →  global  →  built-in default (`DEFAULT_PATHS`).
//!
//! Safety: resolved paths must not contain `..` traversal (which would
//! escape the project root). Absolute paths are now allowed (Task 15.1,
//! post-plan relaxation): they are stored verbatim in the layout but
//! bypass the "join under project root" logic in `validate_wiki_project_root`
//! and `safe_join` — callers that need to combine an absolute path with a
//! project root should check `is_absolute()` first.

use std::path::{Component, Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Schema version of the on-disk `paths.yaml` file. Bumped when the
/// `Paths` struct changes in a non-backwards-compatible way. Project
/// files with a different version are rejected on `open_project`;
/// global files with a different version are silently ignored.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Resolved project layout. Every field is a project-relative path
/// (e.g. `raw/sources`, `wiki/entities`, `schema.md`).
///
/// `#[serde(default)]` lets a `paths.yaml` omit any field — unspecified
/// fields fall back to `Paths::default()` during deserialization, which
/// is what makes partial overrides work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Paths {
    pub raw_root: PathBuf,
    pub raw_assets: PathBuf,
    pub raw_sources: PathBuf,
    pub wiki_root: PathBuf,
    pub wiki_entities: PathBuf,
    pub wiki_concepts: PathBuf,
    pub wiki_sources: PathBuf,
    pub wiki_queries: PathBuf,
    pub wiki_comparisons: PathBuf,
    pub wiki_synthesis: PathBuf,
    pub index: PathBuf,
    pub log: PathBuf,
    pub overview: PathBuf,
    pub schema: PathBuf,
    pub purpose: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        // Built-in layout — must mirror the original hardcoded `dirs`
        // array in `commands/project.rs` so existing projects open
        // unchanged when neither project nor global yaml is present.
        Self {
            raw_root: PathBuf::from("raw"),
            raw_assets: PathBuf::from("raw/assets"),
            raw_sources: PathBuf::from("raw/sources"),
            wiki_root: PathBuf::from("wiki"),
            wiki_entities: PathBuf::from("wiki/entities"),
            wiki_concepts: PathBuf::from("wiki/concepts"),
            wiki_sources: PathBuf::from("wiki/sources"),
            wiki_queries: PathBuf::from("wiki/queries"),
            wiki_comparisons: PathBuf::from("wiki/comparisons"),
            wiki_synthesis: PathBuf::from("wiki/synthesis"),
            index: PathBuf::from("wiki/index.md"),
            log: PathBuf::from("wiki/log.md"),
            overview: PathBuf::from("wiki/overview.md"),
            schema: PathBuf::from("schema.md"),
            purpose: PathBuf::from("purpose.md"),
        }
    }
}

/// On-disk structure of `<project_root>/paths.yaml` and
/// `<app_data_dir>/paths.yaml`. The `version` field is checked against
/// `CURRENT_SCHEMA_VERSION` on load; `paths` holds the (possibly
/// partial) override that the user wrote.
///
/// Officially introduced in Task 4; pulled forward to Task 2 so the
/// Task 2 test (which YAML-deserializes a file with top-level
/// `version: 1` + a `paths:` block) compiles. This is a no-op cost —
/// Task 4 will just move the type definition here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathsFile {
    pub version: u32,
    pub paths: Paths,
}

/// Built-in default project layout. Exposed as a `LazyLock` rather
/// than a `const` because `PathBuf::from` is not `const fn`-stable on
/// our MSRV. Callers can either deref it (`DEFAULT_PATHS.raw_sources`)
/// or clone it (`Paths::clone(&DEFAULT_PATHS)`).
pub static DEFAULT_PATHS: std::sync::LazyLock<Paths> =
    std::sync::LazyLock::new(Paths::default);

/// Combine a project root with a layout path. If the layout path is
/// absolute, return it verbatim (Task 15.1 relaxation). Otherwise join
/// it under the project root.
///
/// Use this anywhere the old code did `root.join(&paths.X)` so we
/// transparently support both project-relative and absolute layouts.
pub fn resolve_under(root: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

// ---------------------------------------------------------------------------
// Resolution (Task 3)
// ---------------------------------------------------------------------------

/// Errors produced by `resolve_paths` and the loaders in Task 4/5.
///
/// `Serialize, Deserialize` so it can cross the Tauri command boundary
/// as a structured value when a frontend calls a `path_config` command
/// (Task 10) and gets back a validation error.
#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
pub enum PathConfigError {
    #[error("path '{0}' must be relative and not contain '..'")]
    UnsafePath(String),
    #[error("path '{0}' is absolute; only project-relative paths are allowed")]
    AbsolutePath(String),
    #[error("io: {0}")]
    Io(String),
    #[error("yaml: {0}")]
    Yaml(String),
    #[error("unsupported schema version {0} (expected {CURRENT_SCHEMA_VERSION})")]
    UnsupportedVersion(u32),
}

/// Reject any path that contains a `..` component. Absolute paths are
/// accepted (Task 15.1 relaxation): they are stored as-is in the layout,
/// so callers that previously assumed "always join under project root"
/// must now guard with `is_absolute()` before combining.
fn validate(path: &Path) -> Result<(), PathConfigError> {
    for c in path.components() {
        if matches!(c, Component::ParentDir) {
            return Err(PathConfigError::UnsafePath(path.display().to_string()));
        }
    }
    Ok(())
}

/// Pick the most specific value for a single path key, layered as:
///   1. project override (if the user wrote something non-default)
///   2. global override   (if the user wrote something non-default)
///   3. built-in default
///
/// KNOWN LIMITATION (called out in the plan, fixed in Task 15): the
/// "is the user override?" signal is "differs from default". So a user
/// who explicitly writes `raw_sources: raw/sources` (matching the
/// built-in default) will get the global override instead. The fix is
/// to track per-field "set vs unset" via `Option<PathBuf>`; deferred
/// to Task 15 per the plan.
fn pick(project: &Path, global: &Path, default: &Path) -> PathBuf {
    if project != default {
        project.to_path_buf()
    } else if global != default {
        global.to_path_buf()
    } else {
        default.to_path_buf()
    }
}

/// Resolve the final `Paths` for an open project by layering the
/// project-level override, the global override, and the built-in
/// default. Validates every resulting path before returning.
///
/// `project = None` means no project-level yaml (or absent file).
/// `global = None`  means no global yaml (or absent file). Either
/// `None` falls through to the next layer.
pub fn resolve_paths(
    project: Option<&Paths>,
    global: Option<&Paths>,
) -> Result<Paths, PathConfigError> {
    let defaults = Paths::default();
    let global = global.unwrap_or(&defaults);
    let project = project.unwrap_or(&defaults);

    // Step 1: take the project override verbatim (it already has the
    // global + default filled in by serde for unspecified fields).
    let project_view = Paths {
        raw_root: project.raw_root.clone(),
        raw_assets: project.raw_assets.clone(),
        raw_sources: project.raw_sources.clone(),
        wiki_root: project.wiki_root.clone(),
        wiki_entities: project.wiki_entities.clone(),
        wiki_concepts: project.wiki_concepts.clone(),
        wiki_sources: project.wiki_sources.clone(),
        wiki_queries: project.wiki_queries.clone(),
        wiki_comparisons: project.wiki_comparisons.clone(),
        wiki_synthesis: project.wiki_synthesis.clone(),
        index: project.index.clone(),
        log: project.log.clone(),
        overview: project.overview.clone(),
        schema: project.schema.clone(),
        purpose: project.purpose.clone(),
    };

    // Step 2: for each field, pick (project, global, default) using
    // the != default heuristic.
    let resolved = Paths {
        raw_root: pick(&project_view.raw_root, &global.raw_root, &defaults.raw_root),
        raw_assets: pick(&project_view.raw_assets, &global.raw_assets, &defaults.raw_assets),
        raw_sources: pick(&project_view.raw_sources, &global.raw_sources, &defaults.raw_sources),
        wiki_root: pick(&project_view.wiki_root, &global.wiki_root, &defaults.wiki_root),
        wiki_entities: pick(&project_view.wiki_entities, &global.wiki_entities, &defaults.wiki_entities),
        wiki_concepts: pick(&project_view.wiki_concepts, &global.wiki_concepts, &defaults.wiki_concepts),
        wiki_sources: pick(&project_view.wiki_sources, &global.wiki_sources, &defaults.wiki_sources),
        wiki_queries: pick(&project_view.wiki_queries, &global.wiki_queries, &defaults.wiki_queries),
        wiki_comparisons: pick(&project_view.wiki_comparisons, &global.wiki_comparisons, &defaults.wiki_comparisons),
        wiki_synthesis: pick(&project_view.wiki_synthesis, &global.wiki_synthesis, &defaults.wiki_synthesis),
        index: pick(&project_view.index, &global.index, &defaults.index),
        log: pick(&project_view.log, &global.log, &defaults.log),
        overview: pick(&project_view.overview, &global.overview, &defaults.overview),
        schema: pick(&project_view.schema, &global.schema, &defaults.schema),
        purpose: pick(&project_view.purpose, &global.purpose, &defaults.purpose),
    };

    // Step 3: validate every resolved path.
    for p in [
        &resolved.raw_root, &resolved.raw_assets, &resolved.raw_sources,
        &resolved.wiki_root, &resolved.wiki_entities, &resolved.wiki_concepts,
        &resolved.wiki_sources, &resolved.wiki_queries, &resolved.wiki_comparisons,
        &resolved.wiki_synthesis, &resolved.index, &resolved.log,
        &resolved.overview, &resolved.schema, &resolved.purpose,
    ] {
        validate(p)?;
    }

    Ok(resolved)
}

// ---------------------------------------------------------------------------
// Loaders / savers (Task 4 — project-level; Task 5 — global)
// ---------------------------------------------------------------------------

/// Load the project-level `paths.yaml` from
/// `<project_root>/paths.yaml`.
///
/// - `Ok(None)`  — file does not exist; caller should fall back to the
///                 next layer (global, then built-in default).
/// - `Ok(Some)`  — file present, version matches, parsed cleanly.
/// - `Err(_)`    — I/O error, YAML syntax error, or version mismatch.
///                 Version mismatch on a *project* file is treated as
///                 a hard error (caller rejects the project); the
///                 global file is more lenient (Task 5).
pub fn load_project_yaml(project_root: &Path) -> Result<Option<PathsFile>, PathConfigError> {
    let p = project_root.join("paths.yaml");
    if !p.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&p).map_err(|e| PathConfigError::Io(e.to_string()))?;
    let file: PathsFile =
        serde_yaml::from_str(&s).map_err(|e| PathConfigError::Yaml(e.to_string()))?;
    if file.version != CURRENT_SCHEMA_VERSION {
        return Err(PathConfigError::UnsupportedVersion(file.version));
    }
    Ok(Some(file))
}

/// Save a `PathsFile` to `<project_root>/paths.yaml`. The write is
/// atomic — we serialize to a `.tmp` sibling in the same directory
/// then `rename` over the target, so a half-written file can never
/// appear on disk.
pub fn save_project_yaml(
    project_root: &Path,
    file: &PathsFile,
) -> Result<(), PathConfigError> {
    let s = serde_yaml::to_string(file).map_err(|e| PathConfigError::Yaml(e.to_string()))?;
    let tmp = project_root.join("paths.yaml.tmp");
    std::fs::write(&tmp, s).map_err(|e| PathConfigError::Io(e.to_string()))?;
    std::fs::rename(&tmp, project_root.join("paths.yaml"))
        .map_err(|e| PathConfigError::Io(e.to_string()))?;
    Ok(())
}

/// Load the global `paths.yaml` from `<app_data_dir>/paths.yaml`.
///
/// This is the "across-all-projects" override file. Differs from
/// `load_project_yaml` in two important ways:
///   1. **Lenient on version mismatch** — if the on-disk version is
///      not `CURRENT_SCHEMA_VERSION`, we log to stderr and return
///      `Ok(None)` (i.e. "treat as no override"). The project-level
///      loader treats version mismatch as a hard error; the global
///      loader is more forgiving because a bad global file would
///      break ALL projects, and a stale global from a previous app
///      version is much more common than a stale per-project file.
///   2. **Path is the dir itself**, not a `.llm-wiki/` subdir — the
///      global file lives at the top of `app_data_dir` to mirror
///      `tauri-plugin-store`'s `app-state.json` convention.
pub fn load_global_yaml_at(
    app_data_dir: &Path,
) -> Result<Option<PathsFile>, PathConfigError> {
    let p = app_data_dir.join("paths.yaml");
    if !p.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&p).map_err(|e| PathConfigError::Io(e.to_string()))?;
    let file: PathsFile =
        serde_yaml::from_str(&s).map_err(|e| PathConfigError::Yaml(e.to_string()))?;
    if file.version != CURRENT_SCHEMA_VERSION {
        eprintln!(
            "[path_config] global paths.yaml schema v{} (expected v{}); ignoring",
            file.version, CURRENT_SCHEMA_VERSION
        );
        return Ok(None);
    }
    Ok(Some(file))
}

/// First-run seed: copy `~/.llm-wiki/paths.yaml` (the user-level
/// master config) to the project-level and/or app-global locations,
/// **only if the target does not already exist**.
///
/// Why: the user maintains one canonical config at the user-home
/// location (`<home>/.llm-wiki/paths.yaml`). On first launch (or
/// first project open) we propagate it to the per-project and
/// app-data locations so the existing loaders (`load_project_yaml`,
/// `load_global_yaml_at`) find it. We never overwrite a target that
/// already has content — once the seed is done, each layer is
/// authoritative on its own and may be edited independently.
///
/// Source:    `<home>/.llm-wiki/paths.yaml`
/// Project:   `<project_root>/paths.yaml`     (only if `project_root` is `Some`)
/// Global:    `<app_data_dir>/paths.yaml`    (only if `app_data_dir` is `Some`)
///
/// Returns the list of target files that were actually written.
/// An empty list means either the source was missing, every supplied
/// target already existed, or no targets were supplied — all of
/// which are the steady-state "no-op" cases.
///
/// The write is a plain `std::fs::write` (not the atomic
/// tmp+rename used elsewhere) because the target file is guaranteed
/// not to exist when this function reaches the write — there is
/// nothing to corrupt on a partial write.
pub fn sync_source_to_layers(
    home_dir: &Path,
    project_root: Option<&Path>,
    app_data_dir: Option<&Path>,
) -> Result<Vec<PathBuf>, PathConfigError> {
    let source = home_dir.join(".llm-wiki").join("paths.yaml");
    if !source.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&source)
        .map_err(|e| PathConfigError::Io(e.to_string()))?;

    let mut wrote = Vec::new();

    if let Some(pr) = project_root {
        let target = pr.join("paths.yaml");
        if !target.exists() {
            std::fs::write(&target, &content)
                .map_err(|e| PathConfigError::Io(e.to_string()))?;
            wrote.push(target);
        }
    }

    if let Some(ad) = app_data_dir {
        let target = ad.join("paths.yaml");
        if !target.exists() {
            // app_data_dir is created by Tauri on first run, but be
            // defensive in case it is missing (e.g. running outside
            // a packaged Tauri shell during tests).
            std::fs::create_dir_all(ad)
                .map_err(|e| PathConfigError::Io(e.to_string()))?;
            std::fs::write(&target, &content)
                .map_err(|e| PathConfigError::Io(e.to_string()))?;
            wrote.push(target);
        }
    }

    Ok(wrote)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_paths_have_expected_layout() {
        assert_eq!(DEFAULT_PATHS.raw_sources.to_str().unwrap(), "raw/sources");
        assert_eq!(DEFAULT_PATHS.wiki_entities.to_str().unwrap(), "wiki/entities");
        assert_eq!(DEFAULT_PATHS.wiki_root.to_str().unwrap(), "wiki");
        assert_eq!(DEFAULT_PATHS.index.to_str().unwrap(), "wiki/index.md");
        assert_eq!(DEFAULT_PATHS.purpose.to_str().unwrap(), "purpose.md");
        assert_eq!(DEFAULT_PATHS.schema.to_str().unwrap(), "schema.md");
    }

    #[test]
    fn pathsfile_deserializes_partial_yaml_with_unspecified_fields_defaulting() {
        // The plan (Task 2 Step 1) wrote this test as:
        //   let parsed: Paths = serde_yaml::from_str(yaml).unwrap();
        //   assert_eq!(parsed.version, 1);
        //   assert_eq!(parsed.paths.raw_sources, ...);
        // but `Paths` has no `version` field and no nested `paths`
        // field — the YAML clearly deserializes to `PathsFile`. The
        // plan's INTENT (test top-level `version` + partial `paths:`
        // override with unspecified fields defaulting) is preserved
        // by deserializing to `PathsFile` instead. The PathsFile type
        // is officially introduced in Task 4; pulling it forward is
        // a no-op.
        let yaml = r#"
version: 1
paths:
  raw_sources: documents/inbox
  wiki_entities: wiki/people
"#;
        let parsed: PathsFile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.paths.raw_sources, PathBuf::from("documents/inbox"));
        assert_eq!(parsed.paths.wiki_entities, PathBuf::from("wiki/people"));
        // unspecified fields default to Paths::default()
        assert_eq!(parsed.paths.wiki_concepts, PathBuf::from("wiki/concepts"));
        assert_eq!(parsed.paths.wiki_root, PathBuf::from("wiki"));
        assert_eq!(parsed.paths.purpose, PathBuf::from("purpose.md"));
    }

    #[test]
    fn resolve_uses_project_override_when_present() {
        let project = Paths {
            raw_sources: PathBuf::from("docs/inbox"),
            ..Paths::default()
        };
        let global = Paths::default();
        let resolved = resolve_paths(Some(&project), Some(&global)).unwrap();
        assert_eq!(resolved.raw_sources, PathBuf::from("docs/inbox"));
        // unspecified → global (which here equals default)
        assert_eq!(resolved.wiki_entities, PathBuf::from("wiki/entities"));
    }

    #[test]
    fn resolve_falls_back_to_global_when_no_project() {
        let global = Paths {
            raw_sources: PathBuf::from("g/in"),
            ..Paths::default()
        };
        let resolved = resolve_paths(None, Some(&global)).unwrap();
        assert_eq!(resolved.raw_sources, PathBuf::from("g/in"));
    }

    #[test]
    fn resolve_falls_back_to_default_when_nothing() {
        let resolved = resolve_paths(None, None).unwrap();
        assert_eq!(resolved.raw_sources, PathBuf::from("raw/sources"));
    }

    #[test]
    fn resolve_rejects_parent_traversal() {
        let bad = Paths {
            raw_sources: PathBuf::from("../escape"),
            ..Paths::default()
        };
        let result = resolve_paths(Some(&bad), None);
        assert!(result.is_err(), "must reject '..' in paths");
    }

    #[test]
    fn resolve_accepts_absolute_paths() {
        // Post-Task 15.1 relaxation: absolute paths are allowed and stored
        // verbatim. Callers (api_server, file_sync) guard with is_absolute()
        // when they need to combine with the project root.
        let abs = Paths {
            raw_sources: PathBuf::from("/tmp/llm-wiki-inbox"),
            wiki_root: PathBuf::from("/var/lib/llm-wiki/wiki"),
            ..Paths::default()
        };
        let result = resolve_paths(Some(&abs), None).unwrap();
        assert_eq!(result.raw_sources, PathBuf::from("/tmp/llm-wiki-inbox"));
        assert_eq!(result.wiki_root, PathBuf::from("/var/lib/llm-wiki/wiki"));
        // Other fields still take the built-in default.
        assert_eq!(result.raw_root, PathBuf::from("raw"));
    }

    #[test]
    fn validate_still_rejects_parent_traversal() {
        // `..` is still rejected even after the absolute-path relaxation,
        // because it can escape any project-relative root.
        let bad = Paths {
            raw_sources: PathBuf::from("../escape"),
            ..Paths::default()
        };
        let result = resolve_paths(Some(&bad), None);
        assert!(result.is_err(), "must still reject '..' in paths");
    }

    #[test]
    fn load_project_yaml_missing_returns_none() {
        let dir = tempdir_unique("pc-missing");
        let result = load_project_yaml(&dir).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_project_yaml_present_parses() {
        let dir = tempdir_unique("pc-present");
        std::fs::write(
            dir.join("paths.yaml"),
            "version: 1\npaths:\n  raw_sources: docs/inbox\n",
        )
        .unwrap();
        let result = load_project_yaml(&dir).unwrap().unwrap();
        assert_eq!(result.paths.raw_sources, PathBuf::from("docs/inbox"));
        assert_eq!(result.version, 1);
    }

    #[test]
    fn load_project_yaml_unsupported_version_fails() {
        let dir = tempdir_unique("pc-version");
        std::fs::write(
            dir.join("paths.yaml"),
            "version: 999\npaths: {}\n",
        )
        .unwrap();
        let result = load_project_yaml(&dir);
        assert!(result.is_err());
    }

    #[test]
    fn save_project_yaml_round_trips() {
        let dir = tempdir_unique("pc-save");
        let cfg = PathsFile {
            version: CURRENT_SCHEMA_VERSION,
            paths: Paths {
                raw_sources: PathBuf::from("docs/inbox"),
                ..Paths::default()
            },
        };
        save_project_yaml(&dir, &cfg).unwrap();
        let loaded = load_project_yaml(&dir).unwrap().unwrap();
        assert_eq!(loaded.paths.raw_sources, PathBuf::from("docs/inbox"));
    }

    #[test]
    fn load_global_yaml_at_custom_path() {
        let dir = tempdir_unique("pc-global");
        std::fs::write(
            dir.join("paths.yaml"),
            "version: 1\npaths:\n  raw_sources: g/inbox\n",
        )
        .unwrap();
        let result = load_global_yaml_at(&dir).unwrap().unwrap();
        assert_eq!(result.paths.raw_sources, PathBuf::from("g/inbox"));
    }

    /// End-to-end integration test for Task 12: the IPC commands the
    /// frontend PathConfigPanel calls (get / set / reset) ultimately
    /// drive this load + save + resolve round-trip. This test is the
    /// single place where the full chain is asserted in one go:
    ///   1. Start with a fresh project dir (no yaml)
    ///   2. Save a custom layout (front-end set_path_config)
    ///   3. Reload the file (front-end get_path_config on next open)
    ///   4. Resolve through the 3-layer pipeline (open_project does
    ///      this in production)
    ///   5. Verify the resolved layout reflects the custom paths
    /// This is the closest the lib-level tests can get to a
    /// true create + set + verify integration test; the actual
    /// Tauri command layer is exercised manually in Task 15.
    #[test]
    fn integration_set_then_reload_then_resolve_uses_custom_layout() {
        let dir = tempdir_unique("pc-integration");
        let project_root = dir.join("my-project");
        std::fs::create_dir_all(&project_root).unwrap();

        // 1. Initial state: no yaml on disk. The simulated
        //    open_project sees no project-level override.
        let project_file = load_project_yaml(&project_root).unwrap();
        assert!(project_file.is_none(), "fresh project should have no yaml");

        // 2. Frontend calls set_path_config with a custom layout.
        let custom = PathsFile {
            version: CURRENT_SCHEMA_VERSION,
            paths: Paths {
                raw_sources: PathBuf::from("documents/inbox"),
                wiki_root: PathBuf::from("notes/wiki"),
                purpose: PathBuf::from("README.md"),
                ..Paths::default()
            },
        };
        save_project_yaml(&project_root, &custom).unwrap();

        // 3. User reopens the project (or front-end re-fetches).
        let reloaded = load_project_yaml(&project_root).unwrap().unwrap();
        assert_eq!(reloaded.paths.raw_sources, PathBuf::from("documents/inbox"));
        assert_eq!(reloaded.paths.wiki_root, PathBuf::from("notes/wiki"));
        assert_eq!(reloaded.paths.purpose, PathBuf::from("README.md"));
        // Unspecified fields should have been filled by serde default
        assert_eq!(reloaded.paths.wiki_entities, PathBuf::from("wiki/entities"));

        // 4 + 5. open_project then resolves through the 3-layer pipeline.
        //    No global yaml in this temp dir, so the resolution is
        //    just project + built-in default.
        let resolved = resolve_paths(Some(&reloaded.paths), None).unwrap();
        assert_eq!(resolved.raw_sources, PathBuf::from("documents/inbox"));
        assert_eq!(resolved.wiki_root, PathBuf::from("notes/wiki"));
        assert_eq!(resolved.purpose, PathBuf::from("README.md"));
        assert_eq!(resolved.wiki_entities, PathBuf::from("wiki/entities"));
    }

    // -----------------------------------------------------------------------
    // sync_source_to_layers
    // -----------------------------------------------------------------------

    #[test]
    fn sync_source_to_layers_missing_source_is_noop() {
        let home = tempdir_unique("sync-no-source");
        // No source file at <home>/.llm-wiki/paths.yaml
        let project = tempdir_unique("sync-no-source-prj");
        let app = tempdir_unique("sync-no-source-app");
        let wrote =
            sync_source_to_layers(&home, Some(&project), Some(&app)).unwrap();
        assert!(wrote.is_empty());
        assert!(!project.join("paths.yaml").exists());
        assert!(!app.join("paths.yaml").exists());
    }

    #[test]
    fn sync_source_to_layers_writes_both_targets_when_missing() {
        let home = tempdir_unique("sync-both");
        std::fs::create_dir_all(home.join(".llm-wiki")).unwrap();
        std::fs::write(
            home.join(".llm-wiki/paths.yaml"),
            "version: 1\npaths:\n  raw_sources: from-home\n",
        )
        .unwrap();
        let project = tempdir_unique("sync-both-prj");
        let app = tempdir_unique("sync-both-app");
        let wrote =
            sync_source_to_layers(&home, Some(&project), Some(&app)).unwrap();
        assert_eq!(wrote.len(), 2);
        assert!(project.join("paths.yaml").exists());
        assert!(app.join("paths.yaml").exists());
        // Both targets contain the source content
        let pr = std::fs::read_to_string(project.join("paths.yaml")).unwrap();
        let gl = std::fs::read_to_string(app.join("paths.yaml")).unwrap();
        assert!(pr.contains("from-home"));
        assert!(gl.contains("from-home"));
    }

    #[test]
    fn sync_source_to_layers_skips_existing_project_target() {
        let home = tempdir_unique("sync-prj-exists");
        std::fs::create_dir_all(home.join(".llm-wiki")).unwrap();
        std::fs::write(
            home.join(".llm-wiki/paths.yaml"),
            "version: 1\npaths:\n  raw_sources: from-home\n",
        )
        .unwrap();
        let project = tempdir_unique("sync-prj-exists-prj");
        // Project target pre-populated with different content
        std::fs::write(
            project.join("paths.yaml"),
            "version: 1\npaths:\n  raw_sources: from-prj\n",
        )
        .unwrap();
        let app = tempdir_unique("sync-prj-exists-app");
        let wrote =
            sync_source_to_layers(&home, Some(&project), Some(&app)).unwrap();
        // Only the global target gets written; project target preserved
        assert_eq!(wrote.len(), 1);
        assert_eq!(wrote[0], app.join("paths.yaml"));
        let pr = std::fs::read_to_string(project.join("paths.yaml")).unwrap();
        assert!(pr.contains("from-prj"));
        assert!(!pr.contains("from-home"));
    }

    #[test]
    fn sync_source_to_layers_steady_state_is_noop() {
        let home = tempdir_unique("sync-steady");
        std::fs::create_dir_all(home.join(".llm-wiki")).unwrap();
        std::fs::write(
            home.join(".llm-wiki/paths.yaml"),
            "version: 1\npaths:\n  raw_sources: from-home\n",
        )
        .unwrap();
        let project = tempdir_unique("sync-steady-prj");
        let app = tempdir_unique("sync-steady-app");
        // Both targets pre-populated — steady state
        std::fs::write(project.join("paths.yaml"), "x: 1\n").unwrap();
        std::fs::write(app.join("paths.yaml"), "x: 1\n").unwrap();
        let wrote =
            sync_source_to_layers(&home, Some(&project), Some(&app)).unwrap();
        assert!(wrote.is_empty());
        // Neither target was modified
        let pr = std::fs::read_to_string(project.join("paths.yaml")).unwrap();
        let gl = std::fs::read_to_string(app.join("paths.yaml")).unwrap();
        assert_eq!(pr, "x: 1\n");
        assert_eq!(gl, "x: 1\n");
    }

    /// Test-only helper: create a uniquely-named temp dir under
    /// `std::env::temp_dir()`. The atomic counter avoids collisions
    /// between parallel test threads.
    fn tempdir_unique(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        // Plan's literal text had `static N: AtomicU64 = AtomicU64 =
        // AtomicU64::new(0);` — a clear typo. Corrected to the
        // single-init form below.
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("llm-wiki-{tag}-{n}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
