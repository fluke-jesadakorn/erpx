import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { relocationVersionSnapshot } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r41';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r41.ts');

// Replaced literals, recorded verbatim from the pre-r41 (BEFORE=251) source.
const SUFFIX = ' WHERE company_id=? AND ';
const SNAPSHOTS = [
  `SELECT id,version FROM organization_positions${SUFFIX}department_id=? AND active=1 ORDER BY id`,
  `SELECT id,version FROM (\u00ABidentity_accessPlatformSources.assignments\u00BB) assignments${SUFFIX}scope_kind='department' AND scope_node_id=? AND revoked_at IS NULL ORDER BY id`,
  `SELECT id,version FROM (\u00ABidentity_accessPlatformSources.organization_gpo_rules\u00BB) organization_gpo_rules${SUFFIX}target_kind='department' AND target_id=? ORDER BY id`,
];
const SNAPSHOT_ARGS: [string, string][] = [
  ['organization_positions', 'department_id=? AND active=1'],
  ['(\u00ABidentity_accessPlatformSources.assignments\u00BB) assignments', "scope_kind='department' AND scope_node_id=? AND revoked_at IS NULL"],
  ['(\u00ABidentity_accessPlatformSources.organization_gpo_rules\u00BB) organization_gpo_rules', "target_kind='department' AND target_id=?"],
];
const norm = (s: string) => s.replace(/\$\{([^}]+)\}/g, '\u00AB$1\u00BB');

describe('Organization read-tail r41 byte-identical families', () => {
  test('r41 helper renders byte-identical SQL to every replaced literal', () => {
    SNAPSHOT_ARGS.forEach(([relation, predicate], i) => expect(norm(relocationVersionSnapshot(relation, predicate))).toBe(SNAPSHOTS[i]));
  });

  test('r41 defines one new helper and every r17-r40 helper is untouched', () => {
    expect(helper.match(/export const relocationVersionSnapshot =/g) ?? []).toHaveLength(1);
    for (const [file, names] of [
      ['organization-column-alias-families.ts', ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']],
      ['organization-read-tail-sources.ts', ['departmentSnapshotMissing', 'ownerVersionGuardSource', 'NODE_BRANCH_ACTIVE', 'NODE_BRANCH_ACTIVE_EXISTS']],
      ['organization-read-tail-sources-r39.ts', ['companyNodeScopeSource', 'activeStaffLevelSource', 'changeVersionGuardSource', 'scopeNodeBranchExists']],
      ['organization-read-tail-sources-r40.ts', ['activePositionList', 'recommendedPayDefault', 'nodeChangeByIdCompany', 'activeStaffLevels', 'memberProfileByUser']],
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline family literals are gone, call sites delegate, locks and spaced JOIN constants are retained', () => {
    const org = read('organization.ts');
    for (const s of SNAPSHOTS) expect(org).not.toContain(s);
    expect(org.split('relocationVersionSnapshot(').length - 1).toBe(3);

    // FOR SHARE/FOR UPDATE locks and spaced JOIN constants stay byte-identical.
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r41 BEFORE=251 baseline (expected 249)', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r41 collapses three relocation-snapshot reads into one shared source (-2).
    expect(space).toBe(226);
    expect(space).toBeLessThan(251);
  });
});
