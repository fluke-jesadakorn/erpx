// Pure test for dedupeTilesByFeature. Verifies no feature is rendered twice.

import {
  getAllTilesWithMeta,
  dedupeTilesByFeature,
  UNIVERSAL_TILES,
} from './_dedupeMirror.mjs';

let pass = 0, fail = 0;
function check(name, ok, extra) {
  if (ok) { pass++; console.log(`✓ ${name}`); }
  else    { fail++; console.log(`✗ ${name}${extra ? ' — ' + extra : ''}`); }
}

const ctx = { expenses: [], prs: [], pos: [], policies: [], currentUser: { id: 1, role_name: 'staff' }, execReport: null };
const all = getAllTilesWithMeta(ctx, 'staff');
const deduped = dedupeTilesByFeature(all);

// 1) No duplicate features
const features = deduped.map((t) => t.feature);
const unique = new Set(features);
check(`unique features after dedupe (${features.length} = ${unique.size})`, features.length === unique.size);

// 2) Even when given duplicate features, dedupeTilesByFeature collapses them.
const syntheticDupes = [
  ...UNIVERSAL_TILES,
  { id: 'synth:cfo-cockpit', feature: 'cockpit', group: 'cockpit', requires: { roles: ['cfo'] }, href: '/cfo-cockpit' },
  { id: 'synth:ceo-cockpit', feature: 'cockpit', group: 'cockpit', requires: { roles: ['ceo'] }, href: '/ceo-cockpit' },
];
const dSyn = dedupeTilesByFeature(syntheticDupes);
const cockpitCount = dSyn.filter((t) => t.feature === 'cockpit').length;
check(`dedupe collapses synthetic cockpit dupes (n=${cockpitCount})`, cockpitCount === 1);

// 3) All shared workspace features dedupe to a single tile
const shared = ['cockpit', 'ledger', 'po', 'policy', 'settings', 'org-chart', 'directory', 'departments', 'access-requests'];
for (const f of shared) {
  const n = deduped.filter((t) => t.feature === f).length;
  check(`feature "${f}" appears once (n=${n})`, n === 1);
}

// 4) Persona-neutral: the 6 stage-specific approval features have been
//    collapsed into ONE universal `approve-expense` tile.
const approveCount = deduped.filter((t) => t.feature === 'approve-expense').length;
check(`universal "approve-expense" tile appears once (n=${approveCount})`, approveCount === 1);

const obsoletePersonaFeatures = ['ao-approval', 'as-approval', 'sup-approval', 'hod-approval', 'am-approval', 'cfo-approval'];
const seenObsolete = deduped.filter((t) => obsoletePersonaFeatures.includes(t.feature)).map((t) => t.feature);
check(`no persona-prefixed approval features (${seenObsolete.join(',') || 'none'})`, seenObsolete.length === 0);

console.log(`\n${pass} pass, ${fail} fail`);
process.exit(fail > 0 ? 1 : 0);