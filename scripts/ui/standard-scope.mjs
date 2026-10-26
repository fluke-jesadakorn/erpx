import { readFileSync } from "node:fs";
const sharedConsumers = JSON.parse(readFileSync(new URL("./shared-consumers.json", import.meta.url), "utf8"));
export const deferredPrefixes = [
  "packages/feature-pms/src/components/", "packages/feature-pos/src/components/", "packages/feature-shared/src/components/hospitality/", "packages/front/src/components/",
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
  "packages/feature-apps/src/routes/holdings/[holdingSlug]/calendar/page.tsx",
  "packages/feature-organization/src/components/organization-chart.tsx", "packages/feature-organization/src/components/organization-team-view.tsx",
  "packages/feature-organization/src/components/organization-person-card.tsx", "packages/feature-organization/src/components/organization-person-details.tsx",
  "packages/feature-people_payroll/src/components/people-organization-workspace.tsx", "packages/feature-people_payroll/src/components/people-team-schedule.tsx",
  "packages/feature-runtime/src/components/executive/chart.tsx", "packages/feature-runtime/src/components/executive/widget.tsx",
  "packages/feature-people_payroll/src/components/people-structure-workspace.tsx", "packages/feature-people_payroll/src/components/organization-draft-workspace.tsx",
  "packages/feature-people_payroll/src/components/organization-explorer.tsx", "packages/feature-people_payroll/src/components/organization-panel-rows.tsx",
  "packages/feature-people_payroll/src/components/people-directory.tsx", "packages/feature-people_payroll/src/components/user-management.tsx",
  "packages/feature-people_payroll/src/components/lifecycle-workspace.tsx", "packages/feature-people_payroll/src/components/roster-workspace.tsx",
  "packages/feature-people_payroll/src/components/organization-chart.tsx", "packages/feature-people_payroll/src/components/organization-draft-panel.tsx",
  "packages/feature-people_payroll/src/components/people-organization-rail.tsx",
  "packages/feature-identity_access/src/components/technology/organization-access-workspace.tsx", "packages/feature-identity_access/src/components/technology/directory-console.tsx",
  "packages/feature-identity_access/src/components/technology/position-console.tsx",
  "packages/feature-work_projection/src/components/calendar/calendar-workspace.tsx", "packages/feature-work_projection/src/components/calendar/calendar-day-timeline.tsx",
  "packages/feature-work_projection/src/components/calendar/personal-time-grid.tsx", "packages/feature-work_projection/src/components/calendar/calendar-event.tsx",
  "packages/feature-work_projection/src/components/calendar/month-grid.tsx", "packages/feature-work_projection/src/components/calendar/calendar-forms.tsx",
  "packages/feature-runtime/src/components/executive/canvas.tsx", "packages/feature-runtime/src/components/executive/resizable-widget.tsx",
  "packages/feature-runtime/src/components/executive/canvas-widget.tsx", "packages/feature-shared/src/components/features/layout-editor.tsx",
  "packages/feature-collaboration/src/components/social-post-content-editor.tsx", "packages/feature-collaboration/src/components/social-advanced-editor.tsx",
];
export const sharedAdapters = [
  // Link forwards its caller's anchor/Next props without presentation. Index
  // callers must choose appearance explicitly; authorization stays server-side.
  // Navigation context adds no DOM, classes or widget dimensions. Actual SSR
  // preserves calendar markup and the null-audience Console destination.
  "packages/workplace/src/components/product-link-context.tsx",
  // Explicit legacy Front appearance and standard Office/Account/Admin default;
  // actual Dialog/Button SSR preserves the unchanged navigation/dirty controller.
  "packages/workplace/src/components/product-navigation-boundary.tsx",
  // Actual owner SSR keeps specialized children byte-identical for absent,
  // active, draining and history controls; visual notices live separately.
  "packages/feature-projects/src/components/purchase-context.tsx",
  "packages/feature-people_payroll/src/components/payroll-purchase-context.tsx",
  // Front reads only DOM-free shared account state; controller/retry UI is separate.
  "packages/feature-identity_access/src/components/account-preferences-context.tsx",
  // DOM-free Social command identity transport; actual provider SSR retains
  // specialized child markup and the exact supplied context value.
  "packages/feature-collaboration/src/components/social-command-context.tsx",
  // Callers choose legacy/standard for both the mention trigger and detail Dialog.
  "packages/feature-collaboration/src/components/social-record-mention.tsx",
  // Existing specialized People editors retain the tested legacy default;
  // standard presentation is explicit and submitted-edit acknowledgement stays owned.
  "packages/feature-people_payroll/src/components/people-form-drawer.tsx",
  // Original Payroll form/context and drawer bodies, imported without the
  // page-wide boundary; specialized Compensation retains explicit legacy props.
  "packages/feature-people_payroll/src/components/payroll-form.tsx",
  "packages/feature-people_payroll/src/components/payroll-form-drawer.tsx",
  // Only the specialized Calendar mounts this legacy-default confirmation;
  // real child/Dialog/Button SSR preserves its existing markup and controls.
  "packages/feature-work_projection/src/components/calendar/calendar-draft-boundary.tsx",
  // Real Calendar fields/submit/completion are shared; specialized Team Schedule
  // explicitly selects legacy controls/CSS while standard Calendar retains its default.
  "packages/feature-work_projection/src/components/calendar/standard-calendar-forms.tsx",
  "packages/workplace/src/components/workspace-link.tsx",
  "packages/workplace/src/components/destination-index.tsx",
  "packages/workplace/src/components/destination-index-view.tsx",
  // Shared record navigation is opt-in at register/list triggers; frozen callers keep their presentation.
  "packages/feature-records/src/components/record-link.tsx", "packages/ui/src/components/record-context.tsx",
  "packages/feature-records/src/components/record-collection.tsx", "packages/feature-records/src/components/pms-entry-detail.tsx", "packages/feature-records/src/components/pms-hotel-sources.tsx",
  // Opt-in workspace host leaves excluded routes on the existing record drawer.
  "packages/feature-records/src/components/record-workspace.tsx",
  "packages/feature-records/src/components/record-surface.tsx", "packages/feature-records/src/components/record-drawer.tsx", "packages/feature-records/src/components/pos-record-detail.tsx",
  "packages/workplace/src/routes/login/login-form.tsx",
  "packages/workplace/src/components/workplace-controls.tsx", "packages/workplace/src/components/app-navigation.tsx",
  "packages/feature-shared/src/components/features/feature-page.tsx", "packages/feature-shared/src/components/features/feature-widget.tsx",
  // Navigation context extracted from feature-page; it preserves children and the legacy cut-page default.
  "packages/feature-shared/src/components/features/feature-environment.tsx",
  "packages/feature-finance/src/components/finance-workspace.tsx", "packages/feature-projects/src/components/project-structure-console.tsx",
  "packages/workplace/src/routes/holdings/[holdingSlug]/account-switcher.tsx", "packages/workplace/src/components/role-select.tsx",
  "packages/feature-finance/src/components/money/currency-provider.tsx",
];
export function scopeFor(path) {
  const route = path.replace(/^packages\/[^/]+\/src\/routes\//, "src/app/").replace(/^apps\/[^/]+\/src\/app\//, "src/app/");
  if (deferredPrefixes.some(prefix => path.startsWith(prefix) || route.startsWith(prefix))) return route.startsWith("src/app/marketing/") ? "deferred-public-marketing" : "deferred-pms-pos";
  if (frozenFiles.includes(path)) return "frozen-specialized";
  if (path.startsWith("packages/ui/src/components/")) return "shared-foundation";
  if (sharedAdapters.includes(path)) return "shared-adapter";
  if (sharedConsumers.includes(path)) return "shared-consumer";
  return "standardize";
}
