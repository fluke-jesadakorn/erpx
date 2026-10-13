import { readFileSync } from "node:fs";
const sharedConsumers = JSON.parse(readFileSync(new URL("./shared-consumers.json", import.meta.url), "utf8"));
export const deferredPrefixes = [
  "src/components/pms/", "src/components/pos/", "src/components/shared/hospitality/", "src/components/front/",
  "src/app/book/", "src/app/front/", "src/app/front-access/", "src/app/marketing/",
  "src/app/holdings/[holdingSlug]/pms/", "src/app/holdings/[holdingSlug]/pos/",
  "src/app/holdings/[holdingSlug]/hospitality/",
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
  // Shared record navigation is opt-in at register/list triggers; frozen callers keep their presentation.
  "src/components/records/record-link.tsx", "src/components/records/record-context.tsx",
  "src/components/records/record-collection.tsx", "src/components/records/pms-entry-detail.tsx", "src/components/records/pms-hotel-sources.tsx",
  "src/components/records/record-surface.tsx", "src/components/records/record-drawer.tsx", "src/components/records/pos-record-detail.tsx",
  "src/app/login/login-form.tsx",
  "src/components/workplace/workplace-controls.tsx", "src/components/workplace/app-navigation.tsx",
  "src/components/shared/features/feature-page.tsx", "src/components/shared/features/feature-widget.tsx",
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
