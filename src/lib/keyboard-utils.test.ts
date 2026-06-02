import { describe, it, expect } from "vitest"
import { isImeComposing, shouldBlockKeyDown } from "./keyboard-utils"

// Build a minimal stand-in for a React KeyboardEvent. Vitest's jsdom
// environment doesn't fire real composition events, so we synthesize
// the two signals isImeComposing inspects.
function ke(opts: {
  isComposing?: boolean
  keyCode?: number
  key?: string
}): React.KeyboardEvent {
  return {
    key: opts.key ?? "Enter",
    keyCode: opts.keyCode ?? 13,
    nativeEvent: { isComposing: opts.isComposing ?? false } as unknown as KeyboardEvent,
  } as unknown as React.KeyboardEvent
}

describe("isImeComposing", () => {
  it("is false for a plain Enter press with no IME activity", () => {
    expect(isImeComposing(ke({ key: "Enter", keyCode: 13 }))).toBe(false)
  })

  it("is true when the W3C `isComposing` flag is set", () => {
    expect(isImeComposing(ke({ isComposing: true, keyCode: 229 }))).toBe(true)
  })

  it("is true when keyCode === 229 even after isComposing has flipped back", () => {
    // The commit-press itself: Chromium reports keyCode 229 but
    // isComposing has already cleared. Without this branch the
    // commit Enter leaks through as a submit.
    expect(isImeComposing(ke({ isComposing: false, keyCode: 229 }))).toBe(true)
  })

  it("is true for non-Enter keys during composition", () => {
    // Defensive: arrow keys, escape, etc. during composition should
    // also be treated as IME-owned by callers that care.
    expect(isImeComposing(ke({ key: "ArrowDown", isComposing: true, keyCode: 229 }))).toBe(true)
  })

  it("is false for Shift+Enter (no IME)", () => {
    expect(isImeComposing(ke({ key: "Enter", keyCode: 13 }))).toBe(false)
  })
})

describe("shouldBlockKeyDown", () => {
  it("blocks Enter when explicit React-level isComposing state is true, even if the event has no IME signal", () => {
    // This is the WebKitGTK / Tauri failure mode the function is
    // designed to defend against: the webview fires keydown with
    // isComposing=false and keyCode=13 on the Enter that commits a
    // candidate, so the per-event check would (wrongly) say "not
    // composing". The explicit React state still says composing,
    // so we block.
    const plainEnter = ke({ key: "Enter", keyCode: 13 })
    expect(shouldBlockKeyDown(true, plainEnter)).toBe(true)
  })

  it("blocks Enter when the per-event isComposing flag is set, even if React state has not yet flipped to composing", () => {
    // Defensive: some webviews emit keydown for the first composing
    // character BEFORE compositionstart has fired, so React state
    // is still false but the per-event flag is true. We must still
    // block.
    const composingKey = ke({ isComposing: true, keyCode: 229, key: "n" })
    expect(shouldBlockKeyDown(false, composingKey)).toBe(true)
  })

  it("blocks Enter when keyCode is 229 (the Chromium commit-press signal) but isComposing has already flipped back", () => {
    const commitPress = ke({ isComposing: false, keyCode: 229 })
    expect(shouldBlockKeyDown(false, commitPress)).toBe(true)
  })

  it("does NOT block a plain Enter when neither React state nor the event signal is set", () => {
    // The happy path: user is typing English, hits Enter, message
    // should send. This must not regress.
    const plainEnter = ke({ key: "Enter", keyCode: 13 })
    expect(shouldBlockKeyDown(false, plainEnter)).toBe(false)
  })

  it("does NOT block Shift+Enter when not composing (newlines should still work)", () => {
    const shiftEnter = ke({ key: "Enter", keyCode: 13 })
    expect(shouldBlockKeyDown(false, shiftEnter)).toBe(false)
  })
})
