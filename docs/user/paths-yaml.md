# Customising the project layout (`paths.yaml`)

By default a wiki project has a fixed on-disk layout:

```
<project>/
├── purpose.md
├── schema.md
├── raw/
│   ├── sources/    ← source text the LLM reads from
│   └── assets/     ← binary attachments (images, PDFs)
└── wiki/
    ├── index.md
    ├── log.md
    ├── overview.md
    ├── entities/
    ├── concepts/
    ├── sources/
    ├── queries/
    ├── comparisons/
    └── synthesis/
```

If this layout doesn't fit your setup (existing folder structure, putting the wiki in a subfolder, splitting raw vs wiki across different mounts, etc.), you can override any subset of the paths with a small YAML file. **Both files are optional** — without them the app uses the built-in defaults.

## Two layers, two locations

| Layer | File | Scope | Read behavior on schema mismatch |
|-------|------|-------|----------------------------------|
| **Project** | `<project>/path.yaml`           | only this project | hard error (project fails to open) |
| **Global**   | `<app_data_dir>/paths.yaml`     | all projects on this machine | warning, then ignored |

Resolution order (highest priority first):

1. Project-level `paths.yaml` (per-project override)
2. Global `paths.yaml` (cross-project default)
3. Built-in defaults (the layout above)

If a key is set in both layers, **project wins**. If a key is set in neither, the built-in default is used.

## Creating a `paths.yaml`

### Option A — through the UI (easiest)

Open the project, go to **Settings → Project paths**, edit any field, click **Save**. The UI writes `path.yaml` at the project root.

The "Reopen to apply" hint means: changes take effect the next time the project is opened. The currently-open project still uses the layout it was opened with until you close and reopen it.

### Option B — by hand

Create the file at the appropriate path:

```bash
mkdir -p /path/to/project/.llm-wiki
$EDITOR /path/to/project/path.yaml
```

The schema is:

```yaml
version: 1
paths:
  raw_sources: documents/inbox
  wiki_root: notes/wiki
  purpose: README.md
  # …any subset of the 15 fields below
```

Only `version` is mandatory. Omit any field you want to keep at default.

## The 15 fields

| Field | Default | What it is |
|-------|---------|------------|
| `raw_root`       | `raw`             | folder that holds all ingested source material |
| `raw_assets`     | `raw/assets`      | subfolder for binary attachments (images, PDFs) |
| `raw_sources`    | `raw/sources`     | subfolder for source text files the LLM reads |
| `wiki_root`      | `wiki`            | folder that holds the generated wiki pages |
| `wiki_entities`  | `wiki/entities`   | named-thing pages (models, people, datasets) |
| `wiki_concepts`  | `wiki/concepts`   | idea/technique pages |
| `wiki_sources`   | `wiki/sources`    | paper/article/talk pages |
| `wiki_queries`   | `wiki/queries`    | open-question pages |
| `wiki_comparisons` | `wiki/comparisons` | side-by-side analysis pages |
| `wiki_synthesis` | `wiki/synthesis`  | cross-cutting summary pages |
| `index`          | `wiki/index.md`   | wiki index markdown (lists all pages grouped by type) |
| `log`            | `wiki/log.md`     | research log markdown (reverse-chronological) |
| `overview`       | `wiki/overview.md`| project overview markdown |
| `schema`         | `schema.md`       | project schema markdown |
| `purpose`        | `purpose.md`      | project purpose markdown |

All paths are **project-relative** by default (no leading slash, no `..`).

> **Absolute paths are allowed** (added in the Task 15.1 relaxation). If a field
> starts with `/` it is used verbatim and **bypasses the project root** —
> useful when the wiki lives outside the project directory, or you want to
> point raw_sources at a shared inbox. Note: absolute paths make the
> `paths.yaml` **non-portable across machines**; only use them when the
> project stays on one machine.

## Common scenarios

### Fit an existing folder structure

```yaml
# path.yaml
version: 1
paths:
  raw_sources: inbox
  wiki_root: my-wiki
  purpose: MISSION.md
```

The project now uses `inbox/` for sources and `my-wiki/` for the generated pages, but the wiki subdirectory layout (`my-wiki/entities/`, `my-wiki/index.md`, …) is unchanged.

### Point the wiki outside the project root (absolute paths)

```yaml
# path.yaml  (or <app_data_dir>/paths.yaml for global)
version: 1
paths:
  wiki_root: /home/me/notes/my-wiki
  raw_sources: /home/me/Dropbox/inbox
  schema:  /home/me/notes/my-wiki/schema.md
  purpose: /home/me/notes/my-wiki/purpose.md
  index:   /home/me/notes/my-wiki/index.md
  log:     /home/me/notes/my-wiki/log.md
  overview: /home/me/notes/my-wiki/overview.md
```

Absolute paths are stored verbatim; they do **not** get joined under the
project root. `..` is still rejected.

### Set a cross-machine default

If every project on this machine should use the same wiki subfolder layout, put it in the global file:

```bash
# Linux
$EDITOR ~/.local/share/com.llmwiki.app/paths.yaml
# macOS
$EDITOR ~/Library/Application\ Support/com.llmwiki.app/paths.yaml
# Windows
notepad %APPDATA%\com.llmwiki.app\paths.yaml
```

```yaml
# paths.yaml (global)
version: 1
paths:
  wiki_root: knowledge-base/wiki
  raw_sources: knowledge-base/sources
```

Any project without a project-level override now inherits this layout.

## Caveats

- **Paths must be project-relative.** Absolute paths (`/etc/foo`) and parent traversal (`../foo`) are rejected with a validation error when the project is opened.
- **Changing the schema version** in your file is rejected on open; the file is treated as belonging to an incompatible app version.
- **The "matches the default" gotcha:** if you write a value that is identical to the built-in default (e.g. `raw_sources: raw/sources`), it is treated as "not overridden" and the global file is consulted for that key. To explicitly assert a value, write something the global could differ on. This is a known minor sharp edge and will be fixed in a future release.
- **Renaming the wiki tree while the project is open:** the change is written to disk but the in-memory layout (used by the running session) keeps the old paths. Close and reopen the project to see the new layout take effect.
- **The lancedb store** (vector search index) lives in `.llm-wiki/lancedb/` and is **not** affected by this file — it's a separate concern that lives outside the user-facing project layout.

## Resetting to defaults

Click **Reset to defaults** in the Settings → Project paths panel to delete the project's `path.yaml`. The app will then use the global file (if any) and the built-in defaults.

To reset the **global** file, just delete it from the path listed under "Common scenarios" above.

## Validation errors

If a `paths.yaml` fails to load (syntax error, schema version mismatch, validation failure), the project fails to open with an error message pointing at the offending file. The fix is to either correct the file or reset it via the settings panel.

## See also

- The plan document: [`docs/plans/2026-06-01-configurable-paths.md`](../plans/2026-06-01-configurable-paths.md) — the design rationale and implementation notes for this feature.
