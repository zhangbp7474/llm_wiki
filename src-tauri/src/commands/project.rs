use std::fs;
use std::path::Path;

use chrono::Local;
use tauri::AppHandle;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

use crate::panic_guard::run_guarded;
use crate::path_config::{self, resolve_under, Paths};
use crate::state::ProjectPathsCache;
use crate::types::wiki::WikiProject;

/// Project directories that `create_project` materializes. Returned as
/// `&Path` (a borrow into the resolved `Paths`) so we don't allocate
/// one PathBuf per entry.
fn dirs_from_paths(p: &Paths) -> Vec<&Path> {
    vec![
        p.raw_root.as_path(),
        p.raw_assets.as_path(),
        p.raw_sources.as_path(),
        p.wiki_root.as_path(),
        p.wiki_entities.as_path(),
        p.wiki_concepts.as_path(),
        p.wiki_sources.as_path(),
        p.wiki_queries.as_path(),
        p.wiki_comparisons.as_path(),
        p.wiki_synthesis.as_path(),
    ]
}

#[tauri::command]
pub fn create_project(
    app: AppHandle,
    state: tauri::State<'_, ProjectPathsCache>,
    name: String,
    path: String,
) -> Result<WikiProject, String> {
    run_guarded("create_project", || {
        create_project_impl(app, state, name, path)
    })
}

fn create_project_impl(
    app: AppHandle,
    state: tauri::State<'_, ProjectPathsCache>,
    name: String,
    path: String,
) -> Result<WikiProject, String> {
    let root = Path::new(&path).join(&name);

    if root.exists() {
        return Err(format!("Directory already exists: '{}'", root.display()));
    }

    // Resolve the project layout for this newly-created project. We
    // don't have a project-level yaml yet (we're creating it!), so
    // `project = None`. The global yaml (if any) still applies, plus
    // the built-in defaults.
    let global = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|d| path_config::load_global_yaml_at(&d).ok().flatten())
        .map(|f| f.paths);
    let paths =
        path_config::resolve_paths(None, global.as_ref()).map_err(|e| format!("path config: {e}"))?;

    // Create all required subdirectories using the resolved layout
    // (not the hardcoded array we used to ship).
    for rel in dirs_from_paths(&paths) {
        fs::create_dir_all(root.join(rel))
            .map_err(|e| format!("Failed to create directory '{}': {}", rel.display(), e))?;
    }

    let today = Local::now().format("%Y-%m-%d").to_string();

    // schema.md — the "Page Types / Directory" table is generated from
    // the resolved layout so the file matches the actual on-disk
    // directory structure even after a yaml override.
    let schema_content = format!(
        r#"# Wiki Schema

## Page Types

| Type | Directory | Purpose |
|------|-----------|---------|
| entity | {ents}/ | Named things (models, companies, people, datasets) |
| concept | {concs}/ | Ideas, techniques, phenomena |
| source | {srcs}/ | Papers, articles, talks, blog posts |
| query | {qrys}/ | Open questions under investigation |
| comparison | {cmps}/ | Side-by-side analysis of related entities |
| synthesis | {syns}/ | Cross-cutting summaries and conclusions |

> If you have an existing project, this template's directory list
> reflects the **built-in default** layout. Your actual layout is
> whatever you (or your project's `paths.yaml`) configured.

## Naming Conventions

- Files: `kebab-case.md`
- Entities: match official name where possible (e.g., `gpt-4.md`, `openai.md`)
- Concepts: descriptive noun phrases (e.g., `chain-of-thought.md`)
- Sources: `author-year-slug.md` (e.g., `wei-2022-chain-of-thought.md`)
- Queries: question as slug (e.g., `does-scale-improve-reasoning.md`)

## Frontmatter

All pages must include YAML frontmatter:

```yaml
---
type: entity | concept | source | query | comparison | synthesis | overview
title: Human-readable title
tags: []
related: []
created: YYYY-MM-DD
updated: YYYY-MM-DD
---
```

Source pages also include:
```yaml
authors: []
year: YYYY
url: ""
venue: ""
```

## Index Format

`{idx}` lists all pages grouped by type. Each entry:
```
- [[page-slug]] — one-line description
```

## Log Format

`{log}` records research activity in reverse chronological order:
```
## YYYY-MM-DD

- Action taken / finding noted
```

## Cross-referencing Rules

- Use `[[page-slug]]` syntax to link between wiki pages
- Every entity and concept should appear in `{idx}`
- Queries link to the sources and concepts they draw on
- Synthesis pages cite all contributing sources via `related:`

## Contradiction Handling

When sources contradict each other:
1. Note the contradiction in the relevant concept or entity page
2. Create or update a query page to track the open question
3. Link both sources from the query page
4. Resolve in a synthesis page once sufficient evidence exists
"#,
        ents = paths.wiki_entities.display(),
        concs = paths.wiki_concepts.display(),
        srcs = paths.wiki_sources.display(),
        qrys = paths.wiki_queries.display(),
        cmps = paths.wiki_comparisons.display(),
        syns = paths.wiki_synthesis.display(),
        idx = paths.index.display(),
        log = paths.log.display(),
    );
    write_file_inner(root.join(&paths.schema), &schema_content)?;

    // purpose.md
    let purpose_content = r#"# Project Purpose

## Goal

<!-- What are you trying to understand or build? -->

## Key Questions

<!-- List the primary questions driving this research -->

1.
2.
3.

## Scope

<!-- What is in scope? What is explicitly out of scope? -->

**In scope:**
-

**Out of scope:**
-

## Thesis

<!-- Your current working hypothesis or conclusion (update as research progresses) -->

> TBD
"#;
    write_file_inner(root.join(&paths.purpose), purpose_content)?;

    // index.md
    let index_content = r#"# Wiki Index

## Entities

## Concepts

## Sources

## Queries

## Comparisons

## Synthesis
"#;
    write_file_inner(root.join(&paths.index), index_content)?;

    // log.md
    let log_content = format!(
        r#"# Research Log

## {today}

- Project created
"#
    );
    write_file_inner(resolve_under(&root, &paths.log), &log_content)?;

    // overview.md
    let overview_content = r#"---
type: overview
title: Project Overview
tags: []
related: []
---

# Overview

<!-- Provide a high-level summary of what this wiki covers and its current state. Update regularly as understanding deepens. -->
"#;
    write_file_inner(root.join(&paths.overview), overview_content)?;

    // .obsidian config for Obsidian compatibility
    fs::create_dir_all(root.join(".obsidian"))
        .map_err(|e| format!("Failed to create .obsidian: {}", e))?;

    // Obsidian app config: set attachment folder (from resolved
    // layout), exclude hidden dirs
    let obsidian_app_config = format!(
        r#"{{
  "attachmentFolderPath": "{assets}",
  "userIgnoreFilters": [
    ".cache",
    ".llm-wiki",
    ".superpowers"
  ],
  "useMarkdownLinks": false,
  "newLinkFormat": "shortest",
  "showUnsupportedFiles": false
}}"#,
        assets = paths.raw_assets.display(),
    );
    write_file_inner(root.join(".obsidian/app.json"), &obsidian_app_config)?;

    // Obsidian appearance: dark mode
    let obsidian_appearance = r#"{
  "baseFontSize": 16,
  "theme": "obsidian"
}"#;
    write_file_inner(root.join(".obsidian/appearance.json"), obsidian_appearance)?;

    // Enable graph view and backlinks core plugins
    let obsidian_core_plugins = r#"{
  "file-explorer": true,
  "global-search": true,
  "graph": true,
  "backlink": true,
  "tag-pane": true,
  "page-preview": true,
  "outgoing-link": true,
  "starred": true
}"#;
    write_file_inner(
        root.join(".obsidian/core-plugins.json"),
        obsidian_core_plugins,
    )?;

    // Cache the resolved layout so subsequent commands (file_sync,
    // vectorstore, etc.) can read it via `tauri::State` instead of
    // re-resolving.
    state.set(paths);

    Ok(WikiProject {
        name,
        // Forward slashes for cross-platform consistency in the TS layer.
        path: root.to_string_lossy().replace('\\', "/"),
    })
}

#[tauri::command]
pub fn open_project(
    app: AppHandle,
    state: tauri::State<'_, ProjectPathsCache>,
    path: String,
) -> Result<WikiProject, String> {
    run_guarded("open_project", || {
        let root = Path::new(&path);

        // Resolve the layout for this project: project-level yaml
        // (if any) + global yaml + built-in defaults. Errors here
        // (version mismatch, parse error, unsafe path) are surfaced
        // to the user as a regular command error.
        let global = app
            .path()
            .app_data_dir()
            .ok()
            .and_then(|d| path_config::load_global_yaml_at(&d).ok().flatten())
            .map(|f| f.paths);
        let project_file = path_config::load_project_yaml(root)
            .map_err(|e| format!("path config: {e}"))?;
        let project_paths = project_file.map(|f| f.paths);
        let paths = path_config::resolve_paths(project_paths.as_ref(), global.as_ref())
            .map_err(|e| format!("path config: {e}"))?;

        validate_wiki_project_root(root, &paths)?;

        // Derive project name from the directory name
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        // Cache the resolved layout for subsequent commands.
        state.set(paths);

        Ok(WikiProject {
            name,
            // Forward slashes for cross-platform consistency in the TS layer.
            path: path.replace('\\', "/"),
        })
    })
}

#[tauri::command]
pub fn open_project_folder(
    app: AppHandle,
    path: String,
) -> Result<(), String> {
    run_guarded("open_project_folder", || {
        let root = Path::new(&path);

        // Re-resolve rather than read from the cache: this command
        // may be called on a project that's not the currently open
        // one (e.g. user right-clicks a different project folder).
        // The cost is one yaml read; the safety is correctness.
        let global = app
            .path()
            .app_data_dir()
            .ok()
            .and_then(|d| path_config::load_global_yaml_at(&d).ok().flatten())
            .map(|f| f.paths);
        let project_file = path_config::load_project_yaml(root)
            .map_err(|e| format!("path config: {e}"))?;
        let project_paths = project_file.map(|f| f.paths);
        let paths = path_config::resolve_paths(project_paths.as_ref(), global.as_ref())
            .map_err(|e| format!("path config: {e}"))?;

        validate_wiki_project_root(root, &paths)?;

        let canonical = root
            .canonicalize()
            .map_err(|e| format!("Failed to resolve project path '{}': {}", path, e))?;
        let canonical = canonical.to_string_lossy().to_string();

        match app.opener().open_path(canonical.clone(), None::<&str>) {
            Ok(()) => Ok(()),
            Err(open_err) => app
                .opener()
                .reveal_item_in_dir(canonical)
                .map_err(|reveal_err| {
                    format!(
                        "Failed to open project folder: {}; reveal fallback also failed: {}",
                        open_err, reveal_err
                    )
                }),
        }
    })
}

fn validate_wiki_project_root(root: &Path, paths: &Paths) -> Result<(), String> {
    if !root.exists() {
        return Err(format!("Path does not exist: '{}'", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("Path is not a directory: '{}'", root.display()));
    }

    // Use the resolved layout, not hardcoded \"schema.md\" / \"wiki\".
    // A project that put its schema at \"docs/schema.md\" via
    // paths.yaml must validate against \"docs/schema.md\".
    if !resolve_under(root, &paths.schema).exists() {
        return Err(format!(
            "Not a valid wiki project (missing {}): '{}'",
            paths.schema.display(),
            root.display()
        ));
    }
    if !resolve_under(root, &paths.wiki_root).is_dir() {
        return Err(format!(
            "Not a valid wiki project (missing {} directory): '{}'",
            paths.wiki_root.display(),
            root.display()
        ));
    }

    Ok(())
}

fn write_file_inner(path: std::path::PathBuf, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create parent dirs for '{}': {}",
                path.display(),
                e
            )
        })?;
    }
    fs::write(&path, contents)
        .map_err(|e| format!("Failed to write file '{}': {}", path.display(), e))
}
