import { readFileSync } from "node:fs";
const sharedConsumers = JSON.parse(readFileSync(new URL("./shared-consumers.json", import.meta.url), "utf8"));
export const deferredPrefixes = [
  "modules/pms/ui/src/components/", "modules/pos/ui/src/components/", "platform/shared/ui/src/components/hospitality/", "platform/front/src/components/",
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
  "products/apps/ui/src/routes/holdings/[holdingSlug]/calendar/page.tsx",
  "modules/organization/ui/src/components/organization-chart.tsx", "modules/organization/ui/src/components/organization-team-view.tsx",
  "modules/organization/ui/src/components/organization-person-card.tsx", "modules/organization/ui/src/components/organization-person-details.tsx",
  "modules/people-payroll/ui/src/components/people-organization-workspace.tsx", "modules/people-payroll/ui/src/components/people-team-schedule.tsx",
  "platform/runtime/ui/src/components/executive/chart.tsx", "platform/runtime/ui/src/components/executive/widget.tsx",
  "modules/people-payroll/ui/src/components/people-structure-workspace.tsx", "modules/people-payroll/ui/src/components/organization-draft-workspace.tsx",
  "modules/people-payroll/ui/src/components/organization-explorer.tsx", "modules/people-payroll/ui/src/components/organization-panel-rows.tsx",
  "modules/people-payroll/ui/src/components/people-directory.tsx", "modules/people-payroll/ui/src/components/user-management.tsx",
  "modules/people-payroll/ui/src/components/lifecycle-workspace.tsx", "modules/people-payroll/ui/src/components/roster-workspace.tsx",
  "modules/people-payroll/ui/src/components/organization-chart.tsx", "modules/people-payroll/ui/src/components/organization-draft-panel.tsx",
  "modules/people-payroll/ui/src/components/people-organization-rail.tsx",
  "modules/identity-access/ui/src/components/technology/organization-access-workspace.tsx", "modules/identity-access/ui/src/components/technology/directory-console.tsx",
  "modules/identity-access/ui/src/components/technology/position-console.tsx",
  "modules/work-projection/ui/src/components/calendar/calendar-workspace.tsx", "modules/work-projection/ui/src/components/calendar/calendar-day-timeline.tsx",
  "modules/work-projection/ui/src/components/calendar/personal-time-grid.tsx", "modules/work-projection/ui/src/components/calendar/calendar-event.tsx",
  "modules/work-projection/ui/src/components/calendar/month-grid.tsx", "modules/work-projection/ui/src/components/calendar/calendar-forms.tsx",
  "platform/runtime/ui/src/components/executive/canvas.tsx", "platform/runtime/ui/src/components/executive/resizable-widget.tsx",
  "platform/runtime/ui/src/components/executive/canvas-widget.tsx", "platform/shared/ui/src/components/features/layout-editor.tsx",
  "modules/collaboration/ui/src/components/social-post-content-editor.tsx", "modules/collaboration/ui/src/components/social-advanced-editor.tsx",
];
export const sharedAdapters = [
  // Link forwards its caller's anchor/Next props without presentation. Index
  // callers must choose appearance explicitly; authorization stays server-side.
  // Navigation context adds no DOM, classes or widget dimensions. Actual SSR
  // preserves calendar markup and the null-audience Console destination.
  "platform/workplace/src/components/product-link-context.tsx",
  // Explicit legacy Front appearance and standard Office/Account/Admin default;
  // actual Dialog/Button SSR preserves the unchanged navigation/dirty controller.
  "platform/workplace/src/components/product-navigation-boundary.tsx",
  // Actual owner SSR keeps specialized children byte-identical for absent,
  // active, draining and history controls; visual notices live separately.
  "modules/projects/ui/src/components/purchase-context.tsx",
  "modules/people-payroll/ui/src/components/payroll-purchase-context.tsx",
  // Front reads only DOM-free shared account state; controller/retry UI is separate.
  "modules/identity-access/ui/src/components/account-preferences-context.tsx",
  // DOM-free Social command identity transport; actual provider SSR retains
  // specialized child markup and the exact supplied context value.
  "modules/collaboration/ui/src/components/social-command-context.tsx",
  // Callers choose legacy/standard for both the mention trigger and detail Dialog.
  "modules/collaboration/ui/src/components/social-record-mention.tsx",
  // Existing specialized People editors retain the tested legacy default;
  // standard presentation is explicit and submitted-edit acknowledgement stays owned.
  "modules/people-payroll/ui/src/components/people-form-drawer.tsx",
  // Original Payroll form/context and drawer bodies, imported without the
  // page-wide boundary; specialized Compensation retains explicit legacy props.
  "modules/people-payroll/ui/src/components/payroll-form.tsx",
  "modules/people-payroll/ui/src/components/payroll-form-drawer.tsx",
  // Only the specialized Calendar mounts this legacy-default confirmation;
  // real child/Dialog/Button SSR preserves its existing markup and controls.
  "modules/work-projection/ui/src/components/calendar/calendar-draft-boundary.tsx",
  // Real Calendar fields/submit/completion are shared; specialized Team Schedule
  // explicitly selects legacy controls/CSS while standard Calendar retains its default.
  "modules/work-projection/ui/src/components/calendar/standard-calendar-forms.tsx",
  "platform/workplace/src/components/workspace-link.tsx",
  "platform/workplace/src/components/destination-index.tsx",
  "platform/workplace/src/components/destination-index-view.tsx",
  // Shared record navigation is opt-in at register/list triggers; frozen callers keep their presentation.
  "modules/records/ui/src/components/record-link.tsx", "platform/ui/src/components/record-context.tsx",
  "modules/records/ui/src/components/record-collection.tsx", "modules/records/ui/src/components/pms-entry-detail.tsx", "modules/records/ui/src/components/pms-hotel-sources.tsx",
  // Opt-in workspace host leaves excluded routes on the existing record drawer.
  "modules/records/ui/src/components/record-workspace.tsx",
  "modules/records/ui/src/components/record-surface.tsx", "modules/records/ui/src/components/record-drawer.tsx", "modules/records/ui/src/components/pos-record-detail.tsx",
  "platform/workplace/src/routes/login/login-form.tsx",
  "platform/workplace/src/components/workplace-controls.tsx", "platform/workplace/src/components/app-navigation.tsx",
  "platform/shared/ui/src/components/features/feature-page.tsx", "platform/shared/ui/src/components/features/feature-widget.tsx",
  // Navigation context extracted from feature-page; it preserves children and the legacy cut-page default.
  "platform/shared/ui/src/components/features/feature-environment.tsx",
  "modules/finance/ui/src/components/finance-workspace.tsx", "modules/projects/ui/src/components/project-structure-console.tsx",
  "platform/workplace/src/routes/holdings/[holdingSlug]/account-switcher.tsx", "platform/workplace/src/components/role-select.tsx",
  "modules/finance/ui/src/components/money/currency-provider.tsx",
];
export function scopeFor(path) {
  const route = path.replace(/^packages\/[^/]+\/src\/routes\//, "src/app/").replace(/^apps\/[^/]+\/src\/app\//, "src/app/");
  if (deferredPrefixes.some(prefix => path.startsWith(prefix) || route.startsWith(prefix))) return route.startsWith("src/app/marketing/") ? "deferred-public-marketing" : "deferred-pms-pos";
  if (frozenFiles.includes(path)) return "frozen-specialized";
  if (path.startsWith("platform/ui/src/components/")) return "shared-foundation";
  if (sharedAdapters.includes(path)) return "shared-adapter";
  if (sharedConsumers.includes(path)) return "shared-consumer";
  return "standardize";
}
