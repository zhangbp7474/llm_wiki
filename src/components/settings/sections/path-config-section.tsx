import { useCallback, useEffect, useState } from "react"
import { useTranslation } from "react-i18next"
import { FolderTree, Loader2, RotateCcw, Save, AlertTriangle, CheckCircle2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { useWikiStore } from "@/stores/wiki-store"
import {
  DEFAULT_PATHS,
  PATHS_FIELDS,
  getPathConfig,
  setPathConfig,
  resetPathConfig,
  type Paths,
  type PathsFile,
} from "@/lib/llm-wiki-paths"

/**
 * Settings section for the project's per-project `paths.yaml` override.
 *
 * Loads the current config (or built-in defaults if no file exists)
 * on mount, lets the user edit any of the 15 path fields, and
 * Save/Reset call the corresponding Tauri commands.
 *
 * IMPORTANT: saving or resetting writes the file, but does NOT
 * reload the open project — the ProjectPathsCache still holds the
 * old layout. The UI surfaces a "Reopen project to apply" hint
 * after every successful write. The plan defers in-process cache
 * invalidation to a follow-up.
 */
export function PathConfigSection() {
  const { t } = useTranslation()
  const project = useWikiStore((s) => s.project)

  const [file, setFile] = useState<PathsFile | null>(null)
  const [original, setOriginal] = useState<PathsFile | null>(null)
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [resetting, setResetting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [info, setInfo] = useState<string | null>(null)

  // Load on mount and whenever the active project changes.
  useEffect(() => {
    if (!project) {
      setFile(null)
      setOriginal(null)
      return
    }
    let cancelled = false
    setLoading(true)
    setError(null)
    setInfo(null)
    void (async () => {
      try {
        const loaded = await getPathConfig(project.path)
        if (cancelled) return
        setFile(loaded)
        setOriginal(loaded)
      } catch (err) {
        if (cancelled) return
        setError(err instanceof Error ? err.message : String(err))
      } finally {
        if (!cancelled) setLoading(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [project])

  const dirty = !!file && !!original && JSON.stringify(file) !== JSON.stringify(original)

  const handleFieldChange = useCallback((key: keyof Paths, value: string) => {
    setFile((prev) => (prev ? { ...prev, paths: { ...prev.paths, [key]: value } } : prev))
  }, [])

  const handleSave = useCallback(async () => {
    if (!project || !file) return
    setSaving(true)
    setError(null)
    setInfo(null)
    try {
      await setPathConfig(project.path, file)
      setOriginal(file)
      setInfo(
        t("settings.sections.pathConfig.saved", {
          defaultValue:
            "Saved. Reopen the project for the new layout to take effect.",
        }),
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }, [project, file, t])

  const handleReset = useCallback(async () => {
    if (!project) return
    setResetting(true)
    setError(null)
    setInfo(null)
    try {
      await resetPathConfig(project.path)
      // After reset, the next getPathConfig returns synthesized defaults.
      const reloaded = await getPathConfig(project.path)
      setFile(reloaded)
      setOriginal(reloaded)
      setInfo(
        t("settings.sections.pathConfig.reset", {
          defaultValue:
            "Reset to defaults. Reopen the project for the new layout to take effect.",
        }),
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setResetting(false)
    }
  }, [project, t])

  if (!project) {
    return (
      <div className="space-y-3">
        <Header t={t} />
        <p className="text-sm text-amber-700 dark:text-amber-400">
          {t("settings.sections.pathConfig.noProject", {
            defaultValue: "Open a project first to edit its paths.yaml.",
          })}
        </p>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <Header t={t} />

      {loading && (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t("settings.sections.pathConfig.loading", { defaultValue: "Loading…" })}
        </div>
      )}

      {error && (
        <div className="flex items-start gap-1.5 rounded border border-rose-500/40 bg-rose-500/5 px-2 py-1.5 text-xs text-rose-700 dark:text-rose-400">
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <div>{error}</div>
        </div>
      )}

      {info && (
        <div className="flex items-start gap-1.5 rounded border border-emerald-500/40 bg-emerald-500/5 px-2 py-1.5 text-xs text-emerald-700 dark:text-emerald-400">
          <CheckCircle2 className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <div>{info}</div>
        </div>
      )}

      {file && (
        <>
          <div className="space-y-3 rounded-lg border border-border/60 bg-muted/10 p-4">
            <p className="text-xs leading-relaxed text-muted-foreground">
              {t("settings.sections.pathConfig.help", {
                defaultValue:
                  "Paths are project-relative (no leading slash, no \"..\"). Changes apply on next project open. Leave a field blank to use the built-in default for that field.",
              })}
            </p>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              {PATHS_FIELDS.map((f) => (
                <div key={f.key} className="space-y-1">
                  <Label htmlFor={`paths-${f.key}`} className="text-xs">
                    {f.label}
                    <code className="ml-1 font-mono text-[10px] text-muted-foreground">
                      ({f.key})
                    </code>
                  </Label>
                  <input
                    id={`paths-${f.key}`}
                    type="text"
                    value={file.paths[f.key]}
                    placeholder={DEFAULT_PATHS[f.key]}
                    onChange={(e) => handleFieldChange(f.key, e.target.value)}
                    disabled={saving || resetting}
                    className="w-full rounded border border-border bg-background px-2 py-1 font-mono text-xs disabled:cursor-not-allowed disabled:opacity-50"
                  />
                  <p className="text-[10px] leading-relaxed text-muted-foreground">
                    {f.description}
                  </p>
                </div>
              ))}
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Button onClick={() => void handleSave()} disabled={!dirty || saving || resetting}>
              {saving ? (
                <>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {t("settings.sections.pathConfig.saving", { defaultValue: "Saving…" })}
                </>
              ) : (
                <>
                  <Save className="h-3.5 w-3.5" />
                  {t("settings.sections.pathConfig.save", { defaultValue: "Save" })}
                </>
              )}
            </Button>
            <Button
              variant="ghost"
              onClick={() => void handleReset()}
              disabled={saving || resetting}
            >
              {resetting ? (
                <>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {t("settings.sections.pathConfig.resetting", { defaultValue: "Resetting…" })}
                </>
              ) : (
                <>
                  <RotateCcw className="h-3.5 w-3.5" />
                  {t("settings.sections.pathConfig.resetButton", {
                    defaultValue: "Reset to defaults",
                  })}
                </>
              )}
            </Button>
            {dirty && (
              <span className="ml-auto text-xs text-amber-700 dark:text-amber-400">
                {t("settings.sections.pathConfig.unsaved", { defaultValue: "Unsaved changes" })}
              </span>
            )}
          </div>
        </>
      )}
    </div>
  )
}

function Header({ t }: { t: ReturnType<typeof useTranslation>["t"] }) {
  return (
    <div>
      <div className="flex items-center gap-2">
        <FolderTree className="h-5 w-5 text-muted-foreground" />
        <h2 className="text-xl font-semibold">
          {t("settings.sections.pathConfig.title", { defaultValue: "Project paths" })}
        </h2>
      </div>
      <p className="mt-1 text-sm text-muted-foreground">
        {t("settings.sections.pathConfig.description", {
          defaultValue:
            "Override the on-disk layout for this project. Writes to path.yaml at the project root. Useful for fitting an existing folder structure or moving the wiki into a subfolder.",
        })}
      </p>
    </div>
  )
}
