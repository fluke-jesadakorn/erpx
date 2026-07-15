// Tile slug coverage tests.
// Run: node app/tests/tile-slug-coverage.test.mjs
//
// Mirrors tileHref + findTileBySlug in app/src/components/tile-config.ts
// Verifies every tile ID produces a valid URL slug, and that the slug resolves
// back to the same tile.

const UNIVERSAL_TILES = [
  // submitter
  { id: 'feature:submit-expense',  feature: 'submit-expense',  subView: 'submit',       requires: { moduleId: 'tile-submit-expense' } },
  { id: 'feature:my-history',      feature: 'my-history',      subView: 'history',      requires: { moduleId: 'tile-my-history' } },
  { id: 'feature:my-prs',          feature: 'my-prs',          requires: { moduleId: 'tile-my-prs' } },
  // approver queues
  { id: 'feature:review-queue',    feature: 'review-queue',    subView: 'queue',        requires: { moduleId: 'tile-review-queue' } },
  { id: 'feature:approve-expense', feature: 'approve-expense', subView: 'approve',      requires: { moduleId: 'tile-approve-expense' } },
  // search
  { id: 'feature:search-coa',      feature: 'search-coa',      subView: 'coa-search',   requires: { moduleId: 'tile-search-coa' } },
  { id: 'feature:search-slips',    feature: 'search-slips',    subView: 'slip-search',  requires: { moduleId: 'tile-search-slips' } },
  // operations
  { id: 'feature:reconciliation',  feature: 'reconciliation',  subView: 'recon',        requires: { moduleId: 'tile-reconciliation' } },
  { id: 'feature:team-manage',     feature: 'team-manage',     subView: 'team-manage',  requires: { moduleId: 'tile-team-manage' } },
  { id: 'feature:ops-overview',    feature: 'ops-overview',    subView: 'ops',          requires: { moduleId: 'tile-ops-overview' } },
  { id: 'feature:workbench',       feature: 'workbench',       requires: { moduleId: 'tile-workbench' } },
  // executive
  { id: 'feature:override-queue',  feature: 'override-queue',  subView: 'override',     requires: { moduleId: 'tile-override-queue' } },
  { id: 'feature:all-approvals',   feature: 'all-approvals',   subView: 'all',          requires: { moduleId: 'tile-all-approvals' } },
  // procurement
  { id: 'feature:subordinate-prs', feature: 'subordinate-prs', requires: { moduleId: 'tile-subordinate-prs' } },
  { id: 'feature:all-prs',         feature: 'all-prs',         requires: { moduleId: 'tile-all-prs' } },
  { id: 'feature:po',              feature: 'po',              subView: 'po',           requires: { moduleId: 'tile-po' } },
  // shared
  { id: 'feature:cockpit',         feature: 'cockpit',         subView: 'main',         requires: { moduleId: 'tile-cockpit' } },
  { id: 'feature:ledger',          feature: 'ledger',          requires: { moduleId: 'tile-ledger' } },
  { id: 'feature:policy',          feature: 'policy',          requires: { moduleId: 'tile-policy' } },
  { id: 'feature:settings',        feature: 'settings',        requires: { moduleId: 'tile-settings' } },
  // HR
  { id: 'feature:org-chart',       feature: 'org-chart',       subView: 'org-chart',    requires: { moduleId: 'tile-org-chart' } },
  { id: 'feature:directory',       feature: 'directory',       subView: 'directory',    requires: { moduleId: 'tile-directory' } },
  { id: 'feature:departments',     feature: 'departments',     subView: 'departments',  requires: { moduleId: 'tile-departments' } },
  { id: 'feature:access-requests', feature: 'access-requests', subView: 'access-requests', requires: { moduleId: 'tile-access-requests' } },
];

function tileHref(id) {
  const [group, slug] = String(id).split(':');
  if (group === 'feature') return '/' + slug;
  return '/' + String(id).replace(/:/g, '-');
}

function findTileBySlug(slug, tiles) {
  const target = '/' + slug;
  return tiles.find((t) => tileHref(t.id) === target) ?? null;
}

// --- Tests ------------------------------------------------------------------

let pass = 0, fail = 0;
function expect(label, actual, expected) {
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  if (ok) { pass++; console.log(`✓ ${label}`); }
  else {
    fail++;
    console.log(`✗ ${label}`);
    console.log(`    expected: ${JSON.stringify(expected)}`);
    console.log(`    actual:   ${JSON.stringify(actual)}`);
  }
}

// 1) Every tile has a non-empty href
console.log('— every tile produces a non-empty href');
for (const t of UNIVERSAL_TILES) {
  const href = tileHref(t.id);
  expect(`${t.id} → href starts with /`, href.startsWith('/'), true);
}

// 2) Hrefs are unique (no two tiles collide)
console.log('\n— hrefs are unique');
const hrefCount = new Map();
for (const t of UNIVERSAL_TILES) {
  const href = tileHref(t.id);
  hrefCount.set(href, (hrefCount.get(href) || 0) + 1);
}
for (const [href, n] of hrefCount.entries()) {
  expect(`href ${href} used by exactly one tile`, n, 1);
}

// 3) Every href resolves back via findTileBySlug
console.log('\n— every href resolves via findTileBySlug');
for (const t of UNIVERSAL_TILES) {
  const href = tileHref(t.id);
  const slug = href.slice(1);
  const resolved = findTileBySlug(slug, UNIVERSAL_TILES);
  expect(`${t.id} (${href}) resolves back`, resolved?.id, t.id);
}

// 4) HR slugs use the global top-level prefix
console.log('\n— HR slugs');
expect('feature:org-chart → /org-chart', tileHref('feature:org-chart'), '/org-chart');
expect('feature:directory → /directory', tileHref('feature:directory'), '/directory');
expect('feature:departments → /departments', tileHref('feature:departments'), '/departments');
expect('feature:access-requests → /access-requests', tileHref('feature:access-requests'), '/access-requests');
expect('findTileBySlug org-chart → feature:org-chart',
  findTileBySlug('org-chart', UNIVERSAL_TILES)?.id,
  'feature:org-chart');

// 5) Unknown slug returns null
console.log('\n— unknown slugs return null');
expect('random slug returns null', findTileBySlug('xyz-abc', UNIVERSAL_TILES), null);
expect('empty slug returns null', findTileBySlug('', UNIVERSAL_TILES), null);

// 6) Every tile has a moduleId for RBAC lookup
console.log('\n— every tile declares a moduleId');
for (const t of UNIVERSAL_TILES) {
  expect(`${t.id} has moduleId`, Boolean(t.requires?.moduleId), true);
}

// 7) Every required moduleId matches the `tile-*` namespace
console.log('\n— moduleIds live in the tile-* namespace');
for (const t of UNIVERSAL_TILES) {
  const mid = t.requires?.moduleId;
  expect(`${t.id} moduleId starts with tile-`, mid?.startsWith('tile-'), true);
}

console.log(`\n${pass} passed, ${fail} failed`);
if (fail > 0) process.exit(1);