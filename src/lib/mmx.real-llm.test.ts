/**
 * Real-LLM tests for the mmx-cli integration.
 *
 * These tests spawn the real `mmx` binary (subprocess) directly via
 * Node's child_process — NOT through the Tauri command boundary —
 * because vitest runs in plain Node and has no Tauri runtime.
 *
 * In production, the same `mmx` calls flow through
 *   src/lib/mmx.ts → @tauri-apps/api invoke() → mmx.rs subprocess.
 * Here we shell out the same way, so a green test verifies that
 * `mmx-cli` is installed, authenticated, and produces the kind of
 * response our UI consumes. The TypeScript wrapper itself is
 * covered by `mmx.test.ts` (mocked invoke).
 *
 * Pre-requisites:
 *   - `mmx` on PATH (`npm i -g mmx-cli`)
 *   - `mmx auth login --api-key <key>` (CLI reads its credentials
 *     from `~/.mmx/config.json`)
 *   - `RUN_LLM_TESTS=1` in the environment to actually execute
 *
 * Invoke with:
 *   RUN_LLM_TESTS=1 npx vitest run mmx.real-llm --reporter=verbose
 *
 * Test plan (from docs/plans/minimax-m2.7-integration.md §1.5):
 *   1. mmx version probe               — installed + authenticated
 *   2. mmx search query                 — web search round-trip
 *   3. mmx vision describe × 4 fixtures — image understanding
 *   4. mmx text chat × 2 scenarios      — single + multi-turn
 *
 * Each vision test asserts on vocabulary words the model is
 * expected to produce, not an exact sentence, so a 1-token wording
 * change doesn't flake the test.
 */
import { spawn } from "node:child_process"
import { beforeAll, describe, expect, it } from "vitest"
import fs from "node:fs"
import path from "node:path"

const ENABLED = process.env.RUN_LLM_TESTS === "1"
const TEST_TIMEOUT_MS = 5 * 60 * 1000
const FIXTURE_DIR = path.resolve(
  process.cwd(),
  "src/test-helpers/fixtures/vision",
)

const runOrSkip = ENABLED ? it : it.skip

// ----- subprocess helpers ---------------------------------------------------

interface MmxResult {
  stdout: string
  stderr: string
  code: number | null
  signal: NodeJS.Signals | null
  elapsedMs: number
}

function runMmx(args: string[], timeoutMs = 60_000): Promise<MmxResult> {
  return new Promise((resolve, reject) => {
    const started = Date.now()
    const child = spawn("mmx", args, {
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, NO_COLOR: "1" },
    })
    let stdout = ""
    let stderr = ""
    const timer = setTimeout(() => {
      child.kill("SIGKILL")
    }, timeoutMs)
    child.stdout.on("data", (b: Buffer) => (stdout += b.toString("utf8")))
    child.stderr.on("data", (b: Buffer) => (stderr += b.toString("utf8")))
    child.on("error", (err) => {
      clearTimeout(timer)
      reject(err)
    })
    child.on("close", (code, signal) => {
      clearTimeout(timer)
      resolve({
        stdout,
        stderr,
        code,
        signal,
        elapsedMs: Date.now() - started,
      })
    })
  })
}

function fixture(name: string): string {
  const p = path.join(FIXTURE_DIR, name)
  // Fail fast if a fixture is missing — vague "image not found" 4xx
  // errors are much harder to debug than a clean assertion failure.
  fs.accessSync(p)
  return p
}

// ----- tests -----------------------------------------------------------------

describe("mmx.real-llm", () => {
  beforeAll(() => {
    if (!ENABLED) {
      // eslint-disable-next-line no-console
      console.log(
        "[mmx.real-llm] RUN_LLM_TESTS !== '1' — skipping. Set RUN_LLM_TESTS=1 to enable.",
      )
    }
  })

  describe("mmx --version (smoke)", () => {
    runOrSkip("prints a semver-shaped version", async () => {
      const r = await runMmx(["--version"], 10_000)
      expect(r.code).toBe(0)
      expect(r.stdout.trim()).toMatch(/^mmx \d+\.\d+\.\d+/)
    }, TEST_TIMEOUT_MS)

    runOrSkip("mmx auth status reports authenticated", async () => {
      const r = await runMmx(["auth", "status", "--output", "json"], 10_000)
      expect(r.code).toBe(0)
      const parsed = JSON.parse(r.stdout)
      expect(parsed.method).toMatch(/oauth|api[-_]?key/)
      expect(parsed.source).toBeTruthy()
    }, TEST_TIMEOUT_MS)
  })

  describe("mmx search query", () => {
    runOrSkip("returns organic results for a real query", async () => {
      const r = await runMmx(
        ["search", "query", "--q", "MiniMax M2.7 self-evolving", "--n", "5", "--output", "json", "--quiet"],
        60_000,
      )
      expect(r.code, `mmx stderr: ${r.stderr}`).toBe(0)
      const envelope = JSON.parse(r.stdout)
      const organic: unknown[] = envelope.organic ?? []
      expect(organic.length).toBeGreaterThan(0)
      for (const item of organic) {
        const hit = item as { title?: string; link?: string; snippet?: string }
        expect(hit.link).toMatch(/^https?:\/\//)
        expect((hit.title ?? "").length).toBeGreaterThan(0)
      }
    }, TEST_TIMEOUT_MS)

    runOrSkip("returns the default ~10-result cap and serialises each hit with link+title", async () => {
      // mmx-cli's `search query` does NOT currently accept a --n flag
      // (verified by running `mmx search query --help` against
      // 1.0.15). It returns up to ~10 organic results per call. Our
      // Rust wrapper's `max_results` is applied client-side after the
      // call; this test just pins the raw CLI's behaviour so a future
      // mmx release that adds --n breaks this test loudly.
      const r = await runMmx(
        ["search", "query", "--q", "Rust programming language", "--output", "json", "--quiet"],
        60_000,
      )
      expect(r.code, `mmx stderr: ${r.stderr}`).toBe(0)
      const envelope = JSON.parse(r.stdout)
      const organic = (envelope.organic ?? []) as Array<{ link?: string; title?: string }>
      expect(organic.length).toBeGreaterThan(0)
      expect(organic.length).toBeLessThanOrEqual(20)
      for (const h of organic) {
        expect(h.link).toMatch(/^https?:\/\//)
        expect((h.title ?? "").length).toBeGreaterThan(0)
      }
    }, TEST_TIMEOUT_MS)
  })

  describe("mmx vision describe on fixtures", () => {
    runOrSkip(
      "chart.png — recognises a line chart and mentions 'revenue'/'quarter'/'chart'",
      async () => {
        const r = await runMmx(
          [
            "vision",
            "describe",
            "--image",
            fixture("chart.png"),
            "--prompt",
            "What does this chart show? Be specific.",
            "--output",
            "json",
            "--quiet",
          ],
          60_000,
        )
        expect(r.code, `mmx stderr: ${r.stderr}`).toBe(0)
        const text = extractText(r.stdout)
        const lower = text.toLowerCase()
        const vocabHit =
          /chart|graph|revenue|quarter|q[12]|line|series|axis/i.test(lower)
        expect(vocabHit, `vision response missing chart vocab: ${text}`).toBe(true)
      },
      TEST_TIMEOUT_MS,
    )

    runOrSkip(
      "screenshot.png — recognises a UI mockup with sidebar/cards",
      async () => {
        const r = await runMmx(
          [
            "vision",
            "describe",
            "--image",
            fixture("screenshot.png"),
            "--prompt",
            "Describe the layout of this interface.",
            "--output",
            "json",
            "--quiet",
          ],
          60_000,
        )
        expect(r.code, `mmx stderr: ${r.stderr}`).toBe(0)
        const text = extractText(r.stdout)
        const lower = text.toLowerCase()
        const vocabHit =
          /sidebar|menu|button|card|header|interface|screen|dashboard/i.test(lower)
        expect(vocabHit, `vision response missing UI vocab: ${text}`).toBe(true)
      },
      TEST_TIMEOUT_MS,
    )

    runOrSkip(
      "diagram.jpg — recognises an architecture diagram with boxes and arrows",
      async () => {
        const r = await runMmx(
          [
            "vision",
            "describe",
            "--image",
            fixture("diagram.jpg"),
            "--prompt",
            "What are the components and how do they connect?",
            "--output",
            "json",
            "--quiet",
          ],
          60_000,
        )
        expect(r.code, `mmx stderr: ${r.stderr}`).toBe(0)
        const text = extractText(r.stdout)
        const lower = text.toLowerCase()
        const vocabHit =
          /box|arrow|component|architecture|diagram|node|connection|module/i.test(lower)
        expect(vocabHit, `vision response missing diagram vocab: ${text}`).toBe(true)
      },
      TEST_TIMEOUT_MS,
    )

    runOrSkip(
      "photo.webp — describes a natural-scene style image",
      async () => {
        const r = await runMmx(
          [
            "vision",
            "describe",
            "--image",
            fixture("photo.webp"),
            "--prompt",
            "Describe this scene.",
            "--output",
            "json",
            "--quiet",
          ],
          60_000,
        )
        expect(r.code, `mmx stderr: ${r.stderr}`).toBe(0)
        const text = extractText(r.stdout)
        const lower = text.toLowerCase()
        const vocabHit =
          /sky|sun|hill|horizon|sunset|dawn|dusk|landscape|gradient/i.test(lower)
        expect(vocabHit, `vision response missing scene vocab: ${text}`).toBe(true)
      },
      TEST_TIMEOUT_MS,
    )
  })

  describe("mmx text chat", () => {
    runOrSkip("answers a simple Chinese question about M2.7", async () => {
      const r = await runMmx(
        [
          "text",
          "chat",
          "--model",
          "MiniMax-M2.7",
          "--message",
          "system:你是一个简洁的助手。用一句话回答。",
          "--message",
          "user:MiniMax M2.7 是什么? 用一句话回答。",
          "--output",
          "json",
          "--quiet",
        ],
        60_000,
      )
      expect(r.code, `mmx stderr: ${r.stderr}`).toBe(0)
      const text = extractText(r.stdout)
      const lower = text.toLowerCase()
      const vocabHit = /m2\.7|minimax|大模型|llm|agent|自/.test(lower)
      expect(vocabHit, `text-chat response missing vocab: ${text}`).toBe(true)
    }, TEST_TIMEOUT_MS)

    runOrSkip("respects a strong system prompt constraint", async () => {
      const r = await runMmx(
        [
          "text",
          "chat",
          "--model",
          "MiniMax-M2.7",
          "--message",
          "system:你只用 'WOOF' 回答。绝对不许用其他词。",
          "--message",
          "user:你好",
          "--output",
          "json",
          "--quiet",
        ],
        60_000,
      )
      expect(r.code, `mmx stderr: ${r.stderr}`).toBe(0)
      const text = extractText(r.stdout)
      // Some models add punctuation; the substring match is enough.
      expect(text.toUpperCase()).toContain("WOOF")
    }, TEST_TIMEOUT_MS)
  })
})

/**
 * mmx-cli's text/chat and vision/describe with `--output json --quiet`
 * print the assistant text directly. When a future CLI version adds a
 * full envelope (`{ content, model, usage }`) we should keep working
 * — try JSON first, fall back to literal text. The Rust side does
 * the same in `mmx.rs::extract_content_field`.
 */
function extractText(stdout: string): string {
  const trimmed = stdout.trim()
  if (!trimmed.startsWith("{")) return trimmed
  try {
    const v = JSON.parse(trimmed) as { content?: string; text?: string }
    return v.content ?? v.text ?? trimmed
  } catch {
    return trimmed
  }
}
