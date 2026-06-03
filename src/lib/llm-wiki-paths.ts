// TypeScript mirror of `src-tauri/src/path_config.rs` (Tasks 1-10).
//
// Tauri 2 auto-converts snake_case Rust field names to camelCase on
// the JS side, so the on-the-wire shape is camelCase. We declare the
// camelCase shape here so the rest of the frontend can use idiomatic
// TS without `as any` casts.
//
// If the Rust struct ever changes (new field, rename, type change),
// update this file in lockstep — Tauri does NOT generate TS types
// from Rust at build time.

import { invoke } from "@tauri-apps/api/core"

/** Resolved project layout. All fields are project-relative paths. */
export interface Paths {
  rawRoot: string
  rawAssets: string
  rawSources: string
  wikiRoot: string
  wikiEntities: string
  wikiConcepts: string
  wikiSources: string
  wikiQueries: string
  wikiComparisons: string
  wikiSynthesis: string
  index: string
  log: string
  overview: string
  schema: string
  purpose: string
}

/** On-disk `paths.yaml` structure: schema version + (possibly partial) Paths. */
export interface PathsFile {
  version: number
  paths: Paths
}

/** Current schema version. Must match `CURRENT_SCHEMA_VERSION` in path_config.rs. */
export const CURRENT_SCHEMA_VERSION = 1

/** Built-in default Paths — also returned by `getPathConfig` when the
 *  project has no `path.yaml`. */
export const DEFAULT_PATHS: Paths = {
  rawRoot: "raw",
  rawAssets: "raw/assets",
  rawSources: "raw/sources",
  wikiRoot: "wiki",
  wikiEntities: "wiki/entities",
  wikiConcepts: "wiki/concepts",
  wikiSources: "wiki/sources",
  wikiQueries: "wiki/queries",
  wikiComparisons: "wiki/comparisons",
  wikiSynthesis: "wiki/synthesis",
  index: "wiki/index.md",
  log: "wiki/log.md",
  overview: "wiki/overview.md",
  schema: "schema.md",
  purpose: "purpose.md",
}

/** Read the current project's `paths.yaml` (or synthesize defaults if absent). */
export async function getPathConfig(projectPath: string): Promise<PathsFile> {
  return invoke<PathsFile>("get_path_config", { projectPath })
}

/** Save a new `paths.yaml` for the project. Atomic write. */
export async function setPathConfig(
  projectPath: string,
  file: PathsFile,
): Promise<void> {
  await invoke("set_path_config", { projectPath, file })
}

/** Delete the project's `paths.yaml` (revert to defaults). */
export async function resetPathConfig(projectPath: string): Promise<void> {
  await invoke("reset_path_config", { projectPath })
}

/** Ordered list of (key, human label, description) for the settings UI. */
export interface PathFieldMeta {
  key: keyof Paths
  label: string
  description: string
}

export const PATHS_FIELDS: readonly PathFieldMeta[] = [
  { key: "rawRoot", label: "Raw root", description: "Folder that holds all ingested source material (raw/ + assets/)." },
  { key: "rawAssets", label: "Raw assets", description: "Subfolder for binary attachments (images, PDFs)." },
  { key: "rawSources", label: "Raw sources", description: "Subfolder for source text files the LLM reads from." },
  { key: "wikiRoot", label: "Wiki root", description: "Folder that holds the generated wiki pages." },
  { key: "wikiEntities", label: "Wiki entities", description: "Subfolder for named-thing pages." },
  { key: "wikiConcepts", label: "Wiki concepts", description: "Subfolder for idea/technique pages." },
  { key: "wikiSources", label: "Wiki sources", description: "Subfolder for paper/article/talk pages." },
  { key: "wikiQueries", label: "Wiki queries", description: "Subfolder for open-question pages." },
  { key: "wikiComparisons", label: "Wiki comparisons", description: "Subfolder for side-by-side analysis pages." },
  { key: "wikiSynthesis", label: "Wiki synthesis", description: "Subfolder for cross-cutting summary pages." },
  { key: "index", label: "Index file", description: "Path (relative to project root) of the wiki index markdown." },
  { key: "log", label: "Log file", description: "Path of the research log markdown (reverse-chronological)." },
  { key: "overview", label: "Overview file", description: "Path of the project overview markdown." },
  { key: "schema", label: "Schema file", description: "Path of the project schema markdown." },
  { key: "purpose", label: "Purpose file", description: "Path of the project purpose markdown." },
] as const

/** True if every field in `file.paths` equals the built-in default. */
export function isAllDefaults(file: PathsFile): boolean {
  for (const { key } of PATHS_FIELDS) {
    if (file.paths[key] !== DEFAULT_PATHS[key]) return false
  }
  return true
}
