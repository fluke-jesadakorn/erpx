# ERPX WCAG 2.2 AA color audit

Date: 2026-09-15. Target: https://erpx.jesadakorn.com. Scope: active application colors, light/dark/system themes. The original audit is preserved below; C1–C3 were subsequently fixed at the user's request.

## Remediation completed — 2026-09-15

- Added `--control-border` (`#7B8178` light, `#858D86` dark and system-dark) and applied it to shared form controls, account fields, social/service/expense/provider forms, messaging controls/composer and the organization search wrapper. Decorative dividers retain their existing colors.
- Set input/textarea placeholders to solid `--muted` with opacity 1.
- Removed the fixed `.error` override so errors use the existing theme-aware `--red`.
- Replaced undefined `--surface`/`--text` form references with `--card`/`--ink`.
- Updated the default asset deployment ID to `wcag-colors-20260915-v1` after browser verification exposed cached pre-fix styles. Fresh public stylesheet URLs use this ID with normal browser caching enabled.

| Verified card-surface pair | Light | Dark | Required |
| --- | --- | --- | --- |
| Placeholder | 5.468:1 | 6.695:1 | 4.5:1 |
| Editable boundary | 3.997:1 | 4.446:1 | 3:1 |
| Error text | 6.271:1 | 7.949:1 | 4.5:1 |

Verification: `bun run typecheck` and `bun run lint` passed. All 25 application CSS files parsed successfully with PostCSS. Unrounded contrast assertions passed for placeholder, control-border and error tokens against paper, card and card-2 in light, dark and system-dark. No remaining input/select/textarea border declarations use the old line tokens.

Public browser checks covered Login, Work filters, Administration feedback, organization search and platform module filters in light/dark, plus system appearance under emulated light/dark preferences. The feedback form was visually checked at desktop and 390 × 844. Error cascade was measured with a temporary `.card > .error` DOM specimen, removed immediately; no business or authentication form was submitted. This verifies styling, not every server validation workflow. The original audit's other coverage limits still apply; this is not a full WCAG certification.

Final local/public health checks returned HTTP 200. Development remains running on port 3100 with the pinned Node/Bun toolchain. Browser appearance is restored to Dark and temporary emulation/viewport overrides are cleared.

## Original audit result

**Color accessibility is not yet AA-ready.** Three shared issues need correction: translucent placeholders, low-contrast input boundaries, and a fixed error-text color that overrides the dark-theme token. The main palette and measured ordinary text generally pass; a wholesale palette replacement is unnecessary.

Inspected 59 distinct URLs in both explicit light and dark themes, plus the platform module index in dark and both system-color modes. Scanned all 25 application CSS files for color declarations and checked related component usage. Visually inspected desktop home/administration and Work at 390 × 844 in both themes using the existing authenticated session.

This does **not** establish complete WCAG conformance or exhaustive coverage of every record, role, interaction, image, or chart state.

## Requirements and method

- Ordinary text, including placeholders: at least 4.5:1. Large text: at least 3:1 (24 CSS px regular or 18⅔ CSS px bold). [W3C SC 1.4.3](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).
- Necessary control boundaries, state indicators and meaningful graphics: at least 3:1 against adjacent colors. Decorative dividers and inactive controls are not automatically failures. [W3C SC 1.4.11](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html).
- Meaning must not depend only on color. Visible text, icons or shapes can supply the alternative. [W3C SC 1.4.1](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html).
- AA requires visible keyboard focus; the additional Focus Appearance area requirement is AAA. [W3C SC 2.4.7](https://www.w3.org/WAI/WCAG22/Understanding/focus-visible.html), [SC 2.4.13](https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html).

Browser measurements used computed styles for visible direct text nodes, walking ancestor backgrounds and compositing transparent layers. Contrast uses WCAG relative luminance: `(Llighter + 0.05) / (Ldarker + 0.05)`, with the sRGB 0.04045 linearization breakpoint. Decisions use unrounded ratios; displayed ratios below are rounded for readability. Disabled controls were excluded. Gradients and ancestor opacity were skipped by the ordinary-text measurement and are not certified by those results. Placeholders and selected form boundaries were inspected separately.

## Findings and exact recommended corrections

| ID | Component / state | Current colors and ratio | Requirement | Recommended correction |
| --- | --- | --- | --- | --- |
| C1 | Work search and Administration feedback placeholders, light | 50% `#414640` over `#FFFFFF`: effective RGB (160, 162.5, 159.5), **2.560:1** | 4.5:1 | Set `input::placeholder, textarea::placeholder` to `color: var(--muted); opacity: 1`. On white this gives **5.468:1**. |
| C1 | Same placeholders, dark | 50% `#DADCD5` over `#23262B`: effective RGB (126.5, 129, 128), **3.864:1** | 4.5:1 | The same semantic rule gives **6.695:1** on dark cards. |
| C2 | Login inputs and Work search, light default state | `--line-strong` `#C4C9C0` against white: **1.685:1** | 3:1 where the boundary identifies the editable area | Add a dedicated `--control-border: #7B8178` for light mode: **3.997:1** against white; **3.632:1** against paper. |
| C2 | Same controls, dark default state | `--line-strong` `#555C5A` against `#23262B`: **2.215:1** | 3:1 | Use `--control-border: #858D86` in dark and system-dark: **4.446:1** against cards; **3.895:1** against card-2. |
| C2 | Administration feedback textarea, both themes | `--line` against same-color card surroundings: light **1.280:1**, dark **1.384:1** | 3:1 | Apply the dedicated control-border token to editable boundaries currently using `--line`, including shared forms. Keep decorative card/divider borders separate. |
| C3 | Shared `.error` messages, especially dark forms; source-confirmed | Later `#BF4646` override against dark paper/card/card-2: **3.432 / 3.021 / 2.646:1**. On light card-2: **4.322:1** | 4.5:1 | Remove the later fixed-color override and retain `color: var(--red)`. Existing dark red `#F1AAA0` gives **7.949:1** on cards; light red `#A43F39` gives **6.271:1** on white. |

### Evidence and affected code

- C1: computed placeholder color on feedback was `oklab(0.387481 -0.00919835 0.00760892 / 0.5)` in light and `oklab(0.890965 -0.00473666 0.00842917 / 0.5)` in dark. Work search used the same dark value. Tailwind's `node_modules/tailwindcss/preflight.css` supplies `color-mix(in oklab, currentcolor 50%, transparent)`; there is no application placeholder override.
- C2: login inputs computed to the `--line-strong` values above. The feedback textarea computed to `--line`, with its fill matching the card behind it; the thin border is needed to identify its editable area. Source locations include the shared `.field`/`.input`, `.business-form`, `.content-stack textarea` rules in `src/app/globals.css` and controls in `src/components/workplace/login.module.css`. Apply the new token to relevant component input/select/textarea/search boundaries as well, then review each control visually. Do not globally darken every decorative `--line` use.
- C3: `src/app/globals.css` first defines `.error` with `var(--red)`, then overrides it near line 539 with `#bf4646`. Shared error messages are used by activation, record editing, commerce, and other business forms. This was confirmed through CSS cascade and component usage; no transaction or deliberately failing server mutation was submitted to expose an error.

## Passing checks and additional source risks

| Pair / behavior | Light | Dark | Assessment |
| --- | --- | --- | --- |
| `--ink` on `--card` | 9.656:1 | 10.971:1 | Ordinary text passes |
| `--muted` on `--paper` | 4.968:1 | 7.606:1 | Ordinary text passes |
| `--muted` on `--card-2` | 4.704:1 | 5.864:1 | Ordinary text passes |
| `--on-brand` on `--brand` | 6.079:1 | 8.085:1 | Primary button text passes |
| Brand focus outline on paper | 5.523:1 | 8.950:1 | Contrast passes; Work type select had a computed 3px solid outline with 3px offset |

No failures were found among the measured ordinary direct text nodes in the explicit light/dark page samples. This excludes the separately measured placeholder failures, skipped content, input values, and states not opened.

Sampled status displays use words such as Waiting, Completed and Needs attention; expense progress includes numbered stages and explanatory text. Administration's selected section includes an underline. These provide cues beyond hue in the inspected states. This is not an exhaustive color-only-meaning audit of every state.

Additional source risks, **not counted as rendered failures**:

- `--muted-2` is only defined, not consumed in current `src`. Its light value `#70766F` would fail on paper (**4.231:1**) and card-2 (**4.006:1**). Avoid using it for small text on those surfaces without a darker replacement.
- `--muted` on light `--warm` is **4.491:1**, which fails 4.5:1 despite rounding to 4.49. The audited avatar usage uses `--ink`, not `--muted`; no current failing use of this pairing was confirmed.
- Shared business-form/company-switcher CSS references undefined `--surface` and `--text` variables. Replace these references with the intended existing `--card` and `--ink` tokens in a follow-up correction; invalid declarations leave appearance dependent on inheritance/browser styles.
- Organization graph edges use `--muted`, while the dotted background uses `--line`. The dots are decorative. Detailed zoomed graph states, gradients and opacity-composited labels still need manual review.

## Coverage

### Workspace: both light and dark

Under `/holdings/acme/`: `home`, `work`, `inbox`, `people`, `people/organization`, `expenses`, `commerce`, `finance`, `finance/invoices`, `finance/ledger`, `finance/bank`, `projects`, `stock-counts`, `stock-transfers`, `handbook`, `learn`, `modules`, and `settings`.

Detail samples: the expense linked from Home's Needs attention section and an invoice linked from the invoice register. Six opacity/complex-background text candidates on the expense and two on the invoice were skipped in each theme. Existing disabled fields/actions were left unchanged.

`requests` and `staff-tasks` lead to Work; legacy staff/routine settings navigation led to Administration. An intermediate staff-settings shell and Cloudflare error pages were excluded from coverage counts.

### Authentication/access: both light and dark

`/login`, `/signup`, `/forgot-password`, `/reset-password`, `/verify-email`, `/activate`, `/join`, `/access`. Token-dependent routes were checked in their token-absent state. No authentication credentials were changed and no forms were submitted.

### Module layouts: both light and dark

All 31 module entry links from `/platform/modules`:

`brand/drafts`, `marketing/campaigns`, `commercial/catalog`, `finance/journals`, `legal/contracts`, `identity_access/people`, `organization/organization-tree`, `platform_administration/service-health`, `agent/agent-catalog`, `collaboration/threads`, `customer_360/profiles`, `customer_authority/bindings`, `support/tickets`, `field_service/dispatch-schedule`, `pos/register`, `sales/quotes`, `crm/pipeline`, `ecommerce/channels`, `assets/asset-register`, `maintenance/work-orders`, `quality/inspections`, `inventory/availability`, `operations/exceptions`, `procurement/requisitions`, `manufacturing/orders`, `planning/scenarios`, `projects/work-breakdown`, `people_payroll/my-requests`, `benefits/available-plans`, `learning/assigned-courses`, `talent/requisitions`.

These platform pages explicitly identify themselves as previews; their results do not establish coverage of every live customer workflow.

The module index was additionally checked with System mode under emulated light/dark preference. Measurements during the 150ms CSS theme transitions were discarded; settled theme measurements found no ordinary-text failures, with minimum ratios of 4.968:1 in system-light and 5.976:1 in system-dark.

### Remaining limits

Not exhaustively verified: every secondary module tab, every dynamic record type, valid invitation/reset-token flows, onboarding, every permission/role combination, all modal/hover/error/validation states, all icon and graph states, user-uploaded images, and every responsive breakpoint. Mobile visual checks used Work as the representative shared shell. No login blocker prevented the sampled checks. No full-app compliance claim is made.

## Verification and development availability

During navigation, the dev server exited following a Turbopack cache-restoration panic (`Restore of All ... restoring failed`). The stopped server's `.next/dev/cache` was moved to an ignored backup under `.scratch/dev`, and `bun run dev` was restarted detached using Node 24.20.0 and Bun 1.2.17. Logs/PID remain under `.scratch/dev`. No application files, Tunnel rules, or production service settings were changed.

Final local and public `/api/health` checks returned HTTP 200 after recovery. Previously failing public routes subsequently rendered ERPX. The public Home page was checked again in the authenticated browser and displayed its greeting, navigation and feed. The original Dark appearance was restored, media emulation was cleared, and the temporary mobile viewport was reset. Development remained listening on port 3100 after its detached startup command had returned.

## Follow-up acceptance checks

1. Apply C1–C3 and resolve the undefined form tokens while preserving the existing palette.
2. Recheck placeholders and empty editable boundaries on Login, Work and Administration in light, dark and system-dark, including mobile.
3. Render a controlled validation-error state for each shared error style without executing business transactions; verify 4.5:1 on its actual surface.
4. Recheck keyboard focus, hover, selected/error states, opacity-composited labels and meaningful chart parts. Keep decorative and disabled elements out of failure counts.
5. Confirm local/public health and public affected-page behavior, leaving development available on port 3100.
