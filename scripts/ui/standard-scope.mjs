import { readFileSync } from "node:fs";
const sharedConsumers = JSON.parse(readFileSync(new URL("./shared-consumers.json", import.meta.url), "utf8"));
export const deferredPrefixes = [
  "src/components/pms/", "src/components/pos/", "src/components/hospitality/", "src/components/front/",
  "src/app/book/", "src/app/front/", "src/app/front-access/", "src/app/marketing/",
  "src/app/holdings/[holdingSlug]/pms/", "src/app/holdings/[holdingSlug]/pos/",
  "src/app/holdings/[holdingSlug]/hospitality/",
];
export const frozenFiles = [
  "src/app/holdings/[holdingSlug]/calendar/page.tsx",
  "src/components/modules/organization-chart.tsx", "src/components/modules/organization-team-view.tsx",
  "src/components/modules/organization-person-card.tsx", "src/components/modules/organization-person-details.tsx",
  "src/components/people/people-organization-workspace.tsx", "src/components/people/people-team-schedule.tsx",
  "src/components/executive/chart.tsx", "src/components/executive/widget.tsx",
  "src/components/people/people-structure-workspace.tsx", "src/components/people/organization-draft-workspace.tsx",
  "src/components/people/organization-explorer.tsx", "src/components/people/organization-panel-rows.tsx",
  "src/components/people/people-directory.tsx", "src/components/people/user-management.tsx",
  "src/components/people/lifecycle-workspace.tsx", "src/components/people/roster-workspace.tsx",
  "src/components/people/organization-chart.tsx", "src/components/people/organization-draft-panel.tsx",
  "src/components/people/people-organization-rail.tsx",
  "src/components/technology/organization-access-workspace.tsx", "src/components/technology/directory-console.tsx",
  "src/components/technology/position-console.tsx",
  "src/components/calendar/calendar-workspace.tsx", "src/components/calendar/calendar-day-timeline.tsx",
  "src/components/calendar/personal-time-grid.tsx", "src/components/calendar/calendar-event.tsx",
  "src/components/calendar/month-grid.tsx", "src/components/calendar/calendar-forms.tsx",
  "src/components/executive/canvas.tsx", "src/components/executive/resizable-widget.tsx",
  "src/components/executive/canvas-widget.tsx", "src/components/features/layout-editor.tsx",
  "src/components/beta/social-post-content-editor.tsx", "src/components/beta/social-advanced-editor.tsx",
];
export const sharedAdapters = [
  "src/app/login/login-form.tsx",
  "src/components/workplace/workplace-controls.tsx", "src/components/workplace/app-navigation.tsx",
  "src/components/features/feature-page.tsx", "src/components/features/feature-widget.tsx",
  "src/components/finance/finance-workspace.tsx", "src/components/modules/project-structure-console.tsx",
  "src/app/holdings/[holdingSlug]/account-switcher.tsx", "src/components/workplace/role-select.tsx",
  "src/components/money/currency-provider.tsx",
];
export function scopeFor(path) {
  if (deferredPrefixes.some(prefix => path.startsWith(prefix))) return path.startsWith("src/app/marketing/") ? "deferred-public-marketing" : "deferred-pms-pos";
  if (frozenFiles.includes(path)) return "frozen-specialized";
  if (path.startsWith("src/components/ui/")) return "shared-foundation";
  if (sharedAdapters.includes(path)) return "shared-adapter";
  if (sharedConsumers.includes(path)) return "shared-consumer";
  return "standardize";
}
