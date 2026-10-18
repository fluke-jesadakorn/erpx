import { readFileSync } from "node:fs";
const sharedConsumers = JSON.parse(readFileSync(new URL("./shared-consumers.json", import.meta.url), "utf8"));
export const deferredPrefixes = [
  "src/components/pms/", "src/components/pos/", "src/components/shared/hospitality/", "src/components/front/",
  "src/app/book/", "src/app/front/", "src/app/front-access/", "src/app/marketing/",
  "src/app/holdings/[holdingSlug]/pms/", "src/app/holdings/[holdingSlug]/pos/",
  "src/app/holdings/[holdingSlug]/hospitality/",
  // Canonical route extraction preserves these presenters' original owner
  // boundary. Their source/retirement fingerprints and browser gates remain.
  "src/app/holdings/[holdingSlug]/_presenters/pms/",
  "src/app/holdings/[holdingSlug]/_presenters/pos/",
  "src/app/holdings/[holdingSlug]/_presenters/hospitality/",
];
export const frozenFiles = [
  "src/app/holdings/[holdingSlug]/calendar/page.tsx",
  "src/components/organization/organization-chart.tsx", "src/components/organization/organization-team-view.tsx",
  "src/components/organization/organization-person-card.tsx", "src/components/organization/organization-person-details.tsx",
  "src/components/people_payroll/people-organization-workspace.tsx", "src/components/people_payroll/people-team-schedule.tsx",
  "src/components/runtime/executive/chart.tsx", "src/components/runtime/executive/widget.tsx",
  "src/components/people_payroll/people-structure-workspace.tsx", "src/components/people_payroll/organization-draft-workspace.tsx",
  "src/components/people_payroll/organization-explorer.tsx", "src/components/people_payroll/organization-panel-rows.tsx",
  "src/components/people_payroll/people-directory.tsx", "src/components/people_payroll/user-management.tsx",
  "src/components/people_payroll/lifecycle-workspace.tsx", "src/components/people_payroll/roster-workspace.tsx",
  "src/components/people_payroll/organization-chart.tsx", "src/components/people_payroll/organization-draft-panel.tsx",
  "src/components/people_payroll/people-organization-rail.tsx",
  "src/components/identity_access/technology/organization-access-workspace.tsx", "src/components/identity_access/technology/directory-console.tsx",
  "src/components/identity_access/technology/position-console.tsx",
  "src/components/work_projection/calendar/calendar-workspace.tsx", "src/components/work_projection/calendar/calendar-day-timeline.tsx",
  "src/components/work_projection/calendar/personal-time-grid.tsx", "src/components/work_projection/calendar/calendar-event.tsx",
  "src/components/work_projection/calendar/month-grid.tsx", "src/components/work_projection/calendar/calendar-forms.tsx",
  "src/components/runtime/executive/canvas.tsx", "src/components/runtime/executive/resizable-widget.tsx",
  "src/components/runtime/executive/canvas-widget.tsx", "src/components/shared/features/layout-editor.tsx",
  "src/components/collaboration/social-post-content-editor.tsx", "src/components/collaboration/social-advanced-editor.tsx",
];
export const sharedAdapters = [
  // Link forwards its caller's anchor/Next props without presentation. Index
  // callers must choose appearance explicitly; authorization stays server-side.
  // Navigation context adds no DOM, classes or widget dimensions. Actual SSR
  // preserves calendar markup and the null-audience Console destination.
  "src/components/workplace/product-link-context.tsx",
  // Explicit legacy Front appearance and standard Office/Account/Admin default;
  // actual Dialog/Button SSR preserves the unchanged navigation/dirty controller.
  "src/components/workplace/product-navigation-boundary.tsx",
  // Actual owner SSR keeps specialized children byte-identical for absent,
  // active, draining and history controls; visual notices live separately.
  "src/components/projects/purchase-context.tsx",
  "src/components/people_payroll/payroll-purchase-context.tsx",
  // Front reads only DOM-free shared account state; controller/retry UI is separate.
  "src/components/identity_access/account-preferences-context.tsx",
  // DOM-free Social command identity transport; actual provider SSR retains
  // specialized child markup and the exact supplied context value.
  "src/components/collaboration/social-command-context.tsx",
  // Callers choose legacy/standard for both the mention trigger and detail Dialog.
  "src/components/collaboration/social-record-mention.tsx",
  // Existing specialized People editors retain the tested legacy default;
  // standard presentation is explicit and submitted-edit acknowledgement stays owned.
  "src/components/people_payroll/people-form-drawer.tsx",
  // Original Payroll form/context and drawer bodies, imported without the
  // page-wide boundary; specialized Compensation retains explicit legacy props.
  "src/components/people_payroll/payroll-form.tsx",
  "src/components/people_payroll/payroll-form-drawer.tsx",
  // Only the specialized Calendar mounts this legacy-default confirmation;
  // real child/Dialog/Button SSR preserves its existing markup and controls.
  "src/components/work_projection/calendar/calendar-draft-boundary.tsx",
  // Real Calendar fields/submit/completion are shared; specialized Team Schedule
  // explicitly selects legacy controls/CSS while standard Calendar retains its default.
  "src/components/work_projection/calendar/standard-calendar-forms.tsx",
  "src/components/workplace/workspace-link.tsx",
  "src/components/workplace/destination-index.tsx",
  "src/components/workplace/destination-index-view.tsx",
  // Shared record navigation is opt-in at register/list triggers; frozen callers keep their presentation.
  "src/components/records/record-link.tsx", "src/components/records/record-context.tsx",
  "src/components/records/record-collection.tsx", "src/components/records/pms-entry-detail.tsx", "src/components/records/pms-hotel-sources.tsx",
  // Opt-in workspace host leaves excluded routes on the existing record drawer.
  "src/components/records/record-workspace.tsx",
  "src/components/records/record-surface.tsx", "src/components/records/record-drawer.tsx", "src/components/records/pos-record-detail.tsx",
  "src/app/login/login-form.tsx",
  "src/components/workplace/workplace-controls.tsx", "src/components/workplace/app-navigation.tsx",
  "src/components/shared/features/feature-page.tsx", "src/components/shared/features/feature-widget.tsx",
  // Navigation context extracted from feature-page; it preserves children and the legacy cut-page default.
  "src/components/shared/features/feature-environment.tsx",
  "src/components/finance/finance-workspace.tsx", "src/components/projects/project-structure-console.tsx",
  "src/app/holdings/[holdingSlug]/account-switcher.tsx", "src/components/workplace/role-select.tsx",
  "src/components/finance/money/currency-provider.tsx",
];
export function scopeFor(path) {
  if (deferredPrefixes.some(prefix => path.startsWith(prefix))) return path.startsWith("src/app/marketing/") ? "deferred-public-marketing" : "deferred-pms-pos";
  if (frozenFiles.includes(path)) return "frozen-specialized";
  if (path.startsWith("src/components/ui/")) return "shared-foundation";
  if (sharedAdapters.includes(path)) return "shared-adapter";
  if (sharedConsumers.includes(path)) return "shared-consumer";
  return "standardize";
}
