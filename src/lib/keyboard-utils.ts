/**
 * Returns true when a keydown event is part of an IME composition
 * (Chinese / Japanese / Korean input methods). When the user is
 * composing — e.g. typing English letters under a Chinese input
 * method to pick a candidate — pressing Enter commits the
 * candidate, but the same Enter keydown ALSO bubbles up to the
 * input element and gets misread by `if (e.key === "Enter")`
 * handlers as a "submit" intent. Result: the message sends
 * before the user actually finished typing.
 *
 * Both signals are required because no single one is reliable:
 *   - `nativeEvent.isComposing` — W3C standard, true while the
 *     IME is composing. Cleared by the time the commit-press
 *     fires in some browsers.
 *   - `keyCode === 229` — the legacy "IME activity" signal that
 *     Chromium continues to emit on the commit-press itself,
 *     after `isComposing` has already flipped back to false.
 *
 * Use this in every Enter-as-submit handler on a text input so
 * IME composition Enter never leaks through as a submit.
 */
export function isImeComposing(e: React.KeyboardEvent): boolean {
  return e.nativeEvent.isComposing || e.keyCode === 229
}

/**
 * Returns true if a keydown event should be BLOCKED from triggering
 * a "submit on Enter" handler because the user is mid-IME composition
 * (typing under a Chinese / Japanese / Korean input method).
 *
 * Why a separate function from `isImeComposing`:
 *   `isImeComposing` only inspects the per-event signals
 *   (`nativeEvent.isComposing`, `keyCode === 229`). Those signals are
 *   NOT reliable in every webview — historically Tauri WebKitGTK on
 *   Linux has been known to omit them on the Enter that commits a
 *   candidate, which leaks the submit through before the user has
 *   finished typing.
 *
 *   This function is meant to be paired with explicit React state
 *   `isComposing` driven by `onCompositionStart` / `onCompositionEnd`
 *   on the input. With that state, the `isComposing || isImeComposing(e)`
 *   short-circuit is a defense-in-depth check: even if the per-event
 *   signal is missing, the React-level state is still true during
 *   composition and blocks the submit.
 *
 * Use this in every Enter-as-submit handler on a text input.
 */
export function shouldBlockKeyDown(
  isComposing: boolean,
  e: React.KeyboardEvent,
): boolean {
  if (isComposing) return true
  return isImeComposing(e)
}
