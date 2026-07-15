// Mirror of the catalog/dedup helpers for the dedupe.test.mjs.
// Mirrors app/src/components/tile-config.ts (universal persona-neutral
// tiles; access driven by RBAC, not by per-tile role allowlists).

const REQ = (tabs, actions) => ({ tabs, actions });

export const UNIVERSAL_TILES = [
  { id: 'feature:submit-expense',  feature: 'submit-expense',  group: 'workflow',              requires: { ...REQ(['workbench'], ['submit_expense']),       moduleId: 'tile-submit-expense' } },
  { id: 'feature:my-history',      feature: 'my-history',      group: 'workflow',              requires: { ...REQ(['workbench'], ['view_own_expenses']),   moduleId: 'tile-my-history' } },
  { id: 'feature:my-prs',          feature: 'my-prs',          group: 'workflow-procurement',  requires: { ...REQ(['pr'],        ['submit_pr']),          moduleId: 'tile-my-prs' } },
  { id: 'feature:review-queue',    feature: 'review-queue',    group: 'workflow-approval',     requires: { ...REQ(['workbench'], ['review_expense']),      moduleId: 'tile-review-queue' } },
  { id: 'feature:approve-expense', feature: 'approve-expense', group: 'workflow-approval',     requires: { ...REQ(['workbench'], ['approve_expense']),     moduleId: 'tile-approve-expense' } },
  { id: 'feature:search-coa',      feature: 'search-coa',      group: 'workflow',              requires: { ...REQ(['workbench'], ['semantic_search']),     moduleId: 'tile-search-coa' } },
  { id: 'feature:search-slips',    feature: 'search-slips',    group: 'workflow',              requires: { ...REQ(['workbench'], ['semantic_search']),     moduleId: 'tile-search-slips' } },
  { id: 'feature:reconciliation',  feature: 'reconciliation',  group: 'finance',               requires: { ...REQ(['workbench'], ['view_ledger']),         moduleId: 'tile-reconciliation' } },
  { id: 'feature:team-manage',     feature: 'team-manage',     group: 'workflow',              requires: { ...REQ(['workbench'], ['assign_role']),         moduleId: 'tile-team-manage' } },
  { id: 'feature:ops-overview',    feature: 'ops-overview',    group: 'it',                    requires: { ...REQ(['workbench'], ['view_all_expenses']),   moduleId: 'tile-ops-overview' } },
  { id: 'feature:override-queue',  feature: 'override-queue',  group: 'cockpit',               requires: { ...REQ(['cockpit'],   ['ceo_override']),       moduleId: 'tile-override-queue' } },
  { id: 'feature:all-approvals',   feature: 'all-approvals',   group: 'cockpit',               requires: { ...REQ(['cockpit'],   ['view_all_expenses']),   moduleId: 'tile-all-approvals' } },
  { id: 'feature:subordinate-prs', feature: 'subordinate-prs', group: 'workflow-procurement',  requires: { ...REQ(['pr'],        ['approve_pr']),         moduleId: 'tile-subordinate-prs' } },
  { id: 'feature:all-prs',         feature: 'all-prs',         group: 'workflow-procurement',  requires: { ...REQ(['pr'],        ['view_po']),            moduleId: 'tile-all-prs' } },
  { id: 'feature:cockpit',         feature: 'cockpit',         group: 'cockpit',               requires: { ...REQ(['cockpit'],   ['view_executive_report']), moduleId: 'tile-cockpit' } },
  { id: 'feature:ledger',          feature: 'ledger',          group: 'finance',               requires: { ...REQ(['ledger'],    ['view_ledger']),         moduleId: 'tile-ledger' } },
  { id: 'feature:po',              feature: 'po',              group: 'workflow-procurement',  requires: { ...REQ(['pr'],        ['approve_po']),         moduleId: 'tile-po' } },
  { id: 'feature:policy',          feature: 'policy',          group: 'policy',                requires: { ...REQ(['policy'],    ['edit_policy']),         moduleId: 'tile-policy' } },
  { id: 'feature:settings',        feature: 'settings',        group: 'it',                    requires: { ...REQ(['settings'],  ['manage_ai_providers']), moduleId: 'tile-settings' } },
  { id: 'feature:org-chart',       feature: 'org-chart',       group: 'hr',                    requires: { ...REQ(['hr'],        ['view_org_chart']),      moduleId: 'tile-org-chart' } },
  { id: 'feature:directory',       feature: 'directory',       group: 'hr',                    requires: { ...REQ(['hr'],        ['view_user_directory']), moduleId: 'tile-directory' } },
  { id: 'feature:departments',     feature: 'departments',     group: 'hr',                    requires: { ...REQ(['hr'],        ['assign_department_head']), moduleId: 'tile-departments' } },
  { id: 'feature:access-requests', feature: 'access-requests', group: 'hr',                    requires: { ...REQ(['hr'],        ['assign_role']),         moduleId: 'tile-access-requests' } },
  { id: 'feature:workbench',       feature: 'workbench',       group: 'it',                    requires: { ...REQ(['workbench'], ['view_all_expenses']),   moduleId: 'tile-workbench' } },
];

export function tileHref(id) {
  const [group, slug] = String(id).split(':');
  if (group === 'feature') return '/' + slug;
  return '/' + String(id).replace(/:/g, '-');
}

export function getAllTilesWithMeta(_ctx, _role) {
  return UNIVERSAL_TILES.map((t) => ({
    ...t,
    href: tileHref(t.id),
  }));
}

const groupOrder = ['workflow', 'workflow-approval', 'workflow-procurement', 'finance', 'cockpit', 'policy', 'it', 'hr'];

export function dedupeTilesByFeature(tiles) {
  const byFeature = new Map();
  for (const t of tiles) {
    if (!byFeature.has(t.feature)) byFeature.set(t.feature, t);
  }
  return Array.from(byFeature.values()).sort(
    (a, b) =>
      groupOrder.indexOf(a.group) - groupOrder.indexOf(b.group) ||
      a.feature.localeCompare(b.feature),
  );
}