/**
 * Frontend wrapper around the `mmx_*` Tauri commands exposed by
 * `src-tauri/src/commands/mmx.rs`. The Rust side does the actual
 * subprocess work; this module is a thin typed proxy so the rest of
 * the app can `import { mmxSearch } from "@/lib/mmx"` without
 * touching `@tauri-apps/api/core` directly.
 *
 * mmx-cli is MiniMax's official CLI for chat, web search, vision,
 * image / video / speech / music generation. We only expose the
 * three subcommands the rest of the app uses — web search (for
 * Deep Research), image understanding (for PDF / Office page
 * captioning and image-aware search), and text chat (for the
 * research synthesis step).
 *
 * Every call goes through Tauri's invoke, which means the calls run
 * in the Rust runtime (not the webview), so they:
 *   - bypass browser CORS / origin checks,
 *   - inherit the proxy env vars set by `set_proxy_env`,
 *   - are timed out by the Rust side (60s default) instead of
 *     hanging the UI on a stuck child process.
 */

import { invoke } from "@tauri-apps/api/core"

/** What `mmx_detect` returns when called from the settings panel. */
export interface MmxDetectResult {
  installed: boolean
  version: string | null
  path: string | null
  error: string | null
}

/** Normalized search hit. Mirrors `WebSearchResult` so it can be fed
 *  into the existing search pipeline without re-mapping. The `date`
 *  field is mmx-specific (other backends don't surface it) and is
 *  the only addition. */
export interface MmxSearchHit {
  title: string
  url: string
  snippet: string
  source: string
  date: string | null
}

/** One message in a chat conversation. Mirrors the Rust `ChatMessage`
 *  struct exactly — keep the field names and casing in sync. */
export interface MmxChatMessage {
  role: "system" | "user" | "assistant"
  content: string
}

/** Probe PATH for `mmx` and report install state + version. Cheap;
 *  safe to call when the settings panel mounts. */
export async function mmxDetect(): Promise<MmxDetectResult> {
  return invoke<MmxDetectResult>("mmx_detect")
}

/** Run a web search through `mmx search query`. */
export async function mmxSearch(
  query: string,
  options: { maxResults?: number; timeoutSecs?: number } = {},
): Promise<MmxSearchHit[]> {
  return invoke<MmxSearchHit[]>("mmx_search", {
    query,
    maxResults: options.maxResults ?? null,
    timeoutSecs: options.timeoutSecs ?? null,
  })
}

/** Describe / ask about an image through `mmx vision describe`.
 *  `imagePath` may be a local path or http(s) URL — mmx-cli handles
 *  both (URLs are fetched server-side by MiniMax, local paths are
 *  base64-encoded by the CLI). */
export async function mmxVisionDescribe(
  imagePath: string,
  options: { prompt?: string; timeoutSecs?: number } = {},
): Promise<string> {
  return invoke<string>("mmx_vision_describe", {
    imagePath,
    prompt: options.prompt ?? null,
    timeoutSecs: options.timeoutSecs ?? null,
  })
}

/** Send a multi-turn chat completion through `mmx text chat`. */
export async function mmxTextChat(
  messages: MmxChatMessage[],
  options: { model?: string; timeoutSecs?: number } = {},
): Promise<string> {
  return invoke<string>("mmx_text_chat", {
    messages,
    model: options.model ?? null,
    timeoutSecs: options.timeoutSecs ?? null,
  })
}
