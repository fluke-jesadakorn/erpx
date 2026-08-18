# macOS native accessibility reproducer

Status: **blocked for macOS release** at revision
`47b0f55bc06caff701d9a0581c2f902ce31749c0`.

This is an isolated acceptance specification for the Dioxus Native renderer. It
does not apply to the Dioxus Web release path, which remains releasable subject
to its own gate. A WebView overlay or fallback is not an acceptable resolution.

## Pinned dependency inspection

The native feature resolves this chain:

```text
erpx-console
  -> dioxus 0.8.0-alpha.1
  -> dioxus-native 0.8.0-alpha.1 (default `accessibility` feature)
  -> blitz-shell 0.3.0-beta.1
  -> blitz-dom 0.3.0-beta.1
  -> accesskit 0.24.1 / accesskit_xplat 0.1.1
```

The result is reproducible with:

```bash
cargo tree -p erpx-console --no-default-features --features native \
  -i blitz-shell --edges normal
```

Static inspection found two independent release blockers:

1. `blitz-shell-0.3.0-beta.1/src/application.rs` receives
   `WindowEvent::ActionRequested`, but its match arm is an empty `TODO`. VoiceOver
   activation, focus, and edit requests therefore never reach the Blitz DOM or
   Dioxus event handlers.
2. `blitz-dom-0.3.0-beta.1/src/accessibility.rs` emits only a small tag-to-role
   mapping plus text-run values and current focus. It does not expose AccessKit
   actions, dialog/select roles, editable control values/selections, or
   selected/expanded/disabled state.

The inspected source SHA-256 values are retained in `status.json`. The crates
are registry dependencies, not vendored ERPX packages. Fixing them requires a
reviewed source pin plus workspace manifest and lockfile changes, and the
missing semantic mapping is broader than a safe one-arm patch. No third-party
source was patched in this audit.

## Reproducer surface

Use one native window containing these controls in DOM order. The exact visible
text is intentional and must not be replaced by English-only test data.

```html
<button id="open-dialog">เปิดกล่องโต้ตอบ</button>
<input id="thai-name" aria-label="ชื่อผู้ใช้" value="สมชาย">
<button id="disclosure" aria-expanded="false">รายละเอียด</button>
<div role="option" aria-selected="true">รายการที่เลือก</div>
<button id="disabled" disabled>บันทึกไม่ได้</button>
<div role="dialog" aria-modal="true" aria-labelledby="dialog-title">
  <h2 id="dialog-title">ยืนยันการทำรายการ</h2>
  <button id="close-dialog">ปิด</button>
</div>
```

The dialog starts closed. Activating `open-dialog` stores that button as the
restore target, opens the dialog, and focuses `close-dialog`. Activating
`close-dialog` closes the dialog and restores focus to `open-dialog`.

## VoiceOver acceptance procedure

Run this only on the release bundle produced by the macOS platform gate. Record
an accessibility transcript and a screen recording under the release revision's
artifact directory, redact account data, and put their digests in the release
evidence record.

1. Start VoiceOver before launching ERPX and navigate to the Thai open button.
   It must be announced as a button with the complete Thai label.
2. Invoke the default action with VoiceOver. The dialog must open, be announced
   as a modal dialog, and move accessibility and keyboard focus to its close
   button exactly once.
3. Invoke the close action. Focus must return to the original open button.
4. Move to the text field, focus it through VoiceOver, replace selected text,
   change the selection, and confirm both the visual value and Dioxus input
   events reflect the edit.
5. Inspect the disclosure before and after activation. VoiceOver must announce
   collapsed then expanded.
6. Inspect the option and disabled button. VoiceOver must announce selected and
   disabled respectively, and must not activate the disabled button.
7. Repeat all actions with keyboard navigation and confirm keyboard focus and
   the AccessKit focus node never diverge.

## Required automated regression seam

Before changing this status to passed, the pinned renderer must have tests that
inject AccessKit requests without using the developer's accessibility settings:

| Request/state | Required observable result |
| --- | --- |
| `Click` on button | one Dioxus click event for the target node |
| `Focus` / `Blur` | DOM focus changes and matching focus events/tree update |
| `ReplaceSelectedText` | value and selection change plus Dioxus input event |
| `SetTextSelection` | bounded selection change on the target text input |
| dialog close | focus restored to the saved invoker |
| selected/expanded/disabled | matching AccessKit node properties |
| Thai accessible name | exact Unicode label, without truncation or replacement |

Passing unit tests is necessary but not sufficient: the manual VoiceOver
procedure above must also pass on the release bundle. Until both layers pass,
macOS native accessibility and the macOS release remain blocked.
