/**
 * Mock tests for the mmx frontend wrapper and the mmx branch of the
 * web-search dispatcher. The Tauri `invoke` is stubbed so we don't
 * need a running Rust side; the goal is to lock down the wire shape
 * and the error translation in `mmxWebSearch`.
 *
 * Real-LLM coverage (sends actual `mmx search query` / `mmx text chat`
 * subprocesses and asserts on the parsed response) lives in
 * `mmx.real-llm.test.ts` and runs only via `npm run test:llm`.
 */
import { beforeEach, describe, expect, it, vi } from "vitest"

// Stub the Tauri invoke module before importing the SUT. The mock
// factory is hoisted by vi.mock, so we configure the spy inside.
const invokeMock = vi.fn()
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

import { mmxDetect, mmxSearch, mmxTextChat, mmxVisionDescribe } from "./mmx"
import { hasConfiguredSearchProvider, webSearch } from "./web-search"

beforeEach(() => {
  invokeMock.mockReset()
})

describe("mmx.ts wrapper", () => {
  it("mmxDetect passes through the Rust result shape", async () => {
    invokeMock.mockResolvedValueOnce({
      installed: true,
      version: "mmx 1.0.15",
      path: "/home/dell/.nvm/versions/node/v22.22.3/bin/mmx",
      error: null,
    })
    const out = await mmxDetect()
    expect(out.installed).toBe(true)
    expect(out.version).toBe("mmx 1.0.15")
    expect(invokeMock).toHaveBeenCalledWith("mmx_detect")
  })

  it("mmxSearch passes query, maxResults, and timeoutSecs to Rust", async () => {
    invokeMock.mockResolvedValueOnce([
      { title: "A", url: "https://a.example/", snippet: "alpha", source: "a.example", date: null },
    ])
    const out = await mmxSearch("alpha", { maxResults: 5, timeoutSecs: 30 })
    expect(invokeMock).toHaveBeenCalledWith("mmx_search", {
      query: "alpha",
      maxResults: 5,
      timeoutSecs: 30,
    })
    expect(out[0].url).toBe("https://a.example/")
  })

  it("mmxSearch omits optional fields when not provided", async () => {
    invokeMock.mockResolvedValueOnce([])
    await mmxSearch("beta")
    expect(invokeMock).toHaveBeenCalledWith("mmx_search", {
      query: "beta",
      maxResults: null,
      timeoutSecs: null,
    })
  })

  it("mmxVisionDescribe passes the image path and prompt", async () => {
    invokeMock.mockResolvedValueOnce("a photo of a cat")
    const out = await mmxVisionDescribe("/tmp/cat.png", { prompt: "What is this?" })
    expect(invokeMock).toHaveBeenCalledWith("mmx_vision_describe", {
      imagePath: "/tmp/cat.png",
      prompt: "What is this?",
      timeoutSecs: null,
    })
    expect(out).toBe("a photo of a cat")
  })

  it("mmxTextChat serialises a multi-turn conversation", async () => {
    invokeMock.mockResolvedValueOnce("the assistant reply")
    const out = await mmxTextChat(
      [
        { role: "system", content: "be terse" },
        { role: "user", content: "hi" },
        { role: "assistant", content: "hello" },
        { role: "user", content: "how are you?" },
      ],
      { model: "MiniMax-M2.7", timeoutSecs: 45 },
    )
    expect(invokeMock).toHaveBeenCalledWith("mmx_text_chat", {
      messages: [
        { role: "system", content: "be terse" },
        { role: "user", content: "hi" },
        { role: "assistant", content: "hello" },
        { role: "user", content: "how are you?" },
      ],
      model: "MiniMax-M2.7",
      timeoutSecs: 45,
    })
    expect(out).toBe("the assistant reply")
  })

  it("propagates Rust errors verbatim", async () => {
    invokeMock.mockRejectedValueOnce(new Error("mmx exited with code Some(3): unauthorized"))
    await expect(mmxSearch("x")).rejects.toThrow(/exited with code/)
  })
})

describe("webSearch with provider=mmx", () => {
  it("returns normalised WebSearchResult[] from mmx hits", async () => {
    invokeMock.mockResolvedValueOnce([
      {
        title: "MiniMax M2.7 介绍",
        url: "https://wallstreetcn.com/livenews/3086511",
        snippet: "M2.7 是业界第一个 AI 深度参与迭代自己的模型",
        source: "wallstreetcn.com",
        date: "2026-04-13 07:00:27",
      },
      {
        title: "Open source release",
        url: "https://github.com/example/m2.7",
        snippet: "Self-evolving AI model",
        source: "github.com",
        date: null,
      },
    ])
    const out = await webSearch(
      "MiniMax M2.7",
      { provider: "mmx", apiKey: "" },
      10,
    )
    expect(out).toEqual([
      {
        title: "MiniMax M2.7 介绍",
        url: "https://wallstreetcn.com/livenews/3086511",
        snippet: "M2.7 是业界第一个 AI 深度参与迭代自己的模型",
        source: "wallstreetcn.com",
      },
      {
        title: "Open source release",
        url: "https://github.com/example/m2.7",
        snippet: "Self-evolving AI model",
        source: "github.com",
      },
    ])
  })

  it("caps the result list at maxResults", async () => {
    invokeMock.mockResolvedValueOnce(
      Array.from({ length: 5 }, (_, i) => ({
        title: `t${i}`,
        url: `https://e.example/${i}`,
        snippet: `s${i}`,
        source: "e.example",
        date: null,
      })),
    )
    const out = await webSearch("q", { provider: "mmx", apiKey: "" }, 2)
    expect(out).toHaveLength(2)
  })

  it("translates 'not found on PATH' to an install hint", async () => {
    invokeMock.mockRejectedValueOnce(new Error("`mmx` not found on PATH (run `npm i -g mmx-cli`)"))
    await expect(
      webSearch("q", { provider: "mmx", apiKey: "" }, 5),
    ).rejects.toThrow(/mmx-cli not found on PATH.*npm i -g mmx-cli/)
  })

  it("translates 'code 3' to an auth hint", async () => {
    invokeMock.mockRejectedValueOnce(new Error("mmx exited with code Some(3): bad token"))
    await expect(
      webSearch("q", { provider: "mmx", apiKey: "" }, 5),
    ).rejects.toThrow(/mmx auth login --api-key/)
  })

  it("translates 'code 4' to a quota hint", async () => {
    invokeMock.mockRejectedValueOnce(new Error("mmx exited with code Some(4): quota exceeded"))
    await expect(
      webSearch("q", { provider: "mmx", apiKey: "" }, 5),
    ).rejects.toThrow(/quota exceeded/)
  })

  it("translates 'code 5' to a timeout hint", async () => {
    invokeMock.mockRejectedValueOnce(new Error("mmx timed out after 60s"))
    await expect(
      webSearch("q", { provider: "mmx", apiKey: "" }, 5),
    ).rejects.toThrow(/timed out/)
  })

  it("leaves unmapped error messages intact", async () => {
    invokeMock.mockRejectedValueOnce(new Error("some other problem"))
    await expect(
      webSearch("q", { provider: "mmx", apiKey: "" }, 5),
    ).rejects.toThrow(/some other problem/)
  })
})

describe("hasConfiguredSearchProvider with mmx", () => {
  it("treats mmx as configured (mmx-cli handles its own auth)", () => {
    expect(
      hasConfiguredSearchProvider({ provider: "mmx", apiKey: "" }),
    ).toBe(true)
  })

  it("still rejects 'none'", () => {
    expect(
      hasConfiguredSearchProvider({ provider: "none", apiKey: "" }),
    ).toBe(false)
  })

  it("still requires a SearXNG URL for the searxng provider", () => {
    expect(
      hasConfiguredSearchProvider({ provider: "searxng", apiKey: "" }),
    ).toBe(false)
    expect(
      hasConfiguredSearchProvider({
        provider: "searxng",
        apiKey: "",
        searXngUrl: "https://search.example.com",
      }),
    ).toBe(true)
  })
})
