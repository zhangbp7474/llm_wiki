//! Project path configuration: parse, resolve, validate.
//!
//! See `docs/plans/2026-06-01-configurable-paths.md` for the design.
//!
//! Two file levels are supported:
//!   1. **Project-level** — `<project_root>/.llm-wiki/paths.yaml` (per-project override)
//!   2. **Global**       — `<app_data_dir>/paths.yaml` (cross-project default)
//!
//! Resolution order for each path key (see `resolve_paths` in Task 3):
//!   project  →  global  →  built-in default (`DEFAULT_PATHS`).
//!
//! Safety: resolved paths must be project-relative and must not contain
//! `..` traversal. This is enforced by `validate` (Task 3).

use std::path::PathBuf;
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

/// On-disk structure of `<project_root>/.llm-wiki/paths.yaml` and
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
}
