import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { activePositionById } from '../../../packages/server-organization/src/adapters/organization-column-alias-families-r38';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families-r38.ts');
const facts = read('people-lifecycle-facts.ts');
const worker = read('people-lifecycle-worker-effects.ts');

// The two inline literals differ only in placeholder style ($1/$2 vs ?).
const FACTS_LITERAL = 'SELECT * FROM organization_positions WHERE company_id=? AND id=? AND active=1';
const WORKER_LITERAL = 'SELECT * FROM organization_positions WHERE company_id=$1 AND id=$2 AND active=1';
// A separate locked statement; it is not part of this family and must not change.
const LOCKED_LITERAL = 'SELECT * FROM organization_positions WHERE company_id=$1 AND id=$2 AND active=1 FOR UPDATE';

describe('Organization read-tail r38 active-position family', () => {
  test('activePositionById renders byte-identical SQL to every replaced literal', () => {
    expect(activePositionById('?', '?')).toBe(FACTS_LITERAL);
    expect(activePositionById('$1', '$2')).toBe(WORKER_LITERAL);
    expect(activePositionById('$1', '$2')).not.toBe(LOCKED_LITERAL);
  });

  test('r38 defines one new helper and every r17-r37 helper is untouched', () => {
    expect(helper.match(/export const activePositionById =/g) ?? []).toHaveLength(1);
    const prior = read('organization-column-alias-families.ts');
    const r27 = read('organization-column-alias-families-r27.ts');
    const r28 = read('organization-column-alias-families-r28.ts');
    const r29 = read('organization-column-alias-families-r29.ts');
    const r30 = read('organization-column-alias-families-r30.ts');
    const r31 = read('organization-column-alias-families-r31.ts');
    const r34 = read('organization-column-alias-families-r34.ts');
    const r35 = read('organization-column-alias-families-r35.ts');
    const r36 = read('organization-column-alias-families-r36.ts');
    const r37 = read('organization-column-alias-families-r37.ts');
    for (const name of ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']) {
      expect(prior.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['activeNodeByKind', 'twoColumnLookup', 'nodeKindByIdCompany', 'versionedNodeActiveGuard']) {
      expect(r27.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['companySnapshotAggregate', 'correlatedNodeJson']) {
      expect(r28.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['maxVersionSubquery', 'impactCount']) {
      expect(r29.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    expect(r30.match(/export const activeLevelAbsent =/g) ?? []).toHaveLength(1);
    expect(r31.match(/export const companyNodeByIdKind =/g) ?? []).toHaveLength(1);
    expect(r34.match(/export const activeNodeKindExists =/g) ?? []).toHaveLength(1);
    expect(r35.match(/export const ownerProjection =/g) ?? []).toHaveLength(1);
    expect(r36.match(/export const activeNodeByIdCompanyKind =/g) ?? []).toHaveLength(1);
    expect(r37.match(/export const leavePolicyVersionLookup =/g) ?? []).toHaveLength(1);
  });

  test('inline family literals are gone, sources delegate, locks and spaced JOIN constants are retained', async () => {
    expect(facts).not.toContain(FACTS_LITERAL);
    expect(worker).not.toContain(WORKER_LITERAL);
    expect(facts.split('activePositionById(').length - 1).toBeGreaterThanOrEqual(1);
    expect(worker.split('activePositionById(').length - 1).toBeGreaterThanOrEqual(1);
    // The FOR UPDATE lock is a separate statement and stays byte-identical.
    expect(facts).toContain(LOCKED_LITERAL);
    // Spaced JOIN constants are unchanged.
    const spaced = await import('../../../packages/server-organization/src/adapters/organization-nodes-spaced-projections');
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'c')).toBe(spaced.ORG_NODES_SPACED_C_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'n')).toBe(spaced.ORG_NODES_SPACED_N_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'b')).toBe(spaced.ORG_NODES_SPACED_B_JOIN);
    expect(spaced.ORG_NODES_SPACED_C_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c');
  });

  test('retirement:ts-statements is strictly below the r38 BEFORE=263 baseline', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r38 collapsed a new 2-literal family into 1 helper statement (net -1)
    // below the task BEFORE=263; no other owner path is touched.
    expect(space).toBeLessThanOrEqual(262);
    expect(space).toBeLessThan(263);
  });
});
