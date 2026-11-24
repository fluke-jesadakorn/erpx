import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { ownerProjection } from '../../../packages/server-organization/src/adapters/organization-column-alias-families-r35';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families-r35.ts');
const finalRoot = read('final-root-owner-projection-sources.ts');
const lastAudit = read('last-audit-owner-projection-sources.ts');
const org = read('org-sources.ts');
const platform = read('platform-owner-projection-sources.ts');
const talent = read('talent-settlement-sources.ts');

describe('Organization read-tail r35 bare owner projection family', () => {
  test('ownerProjection renders byte-identical SQL to every replaced literal', () => {
    expect(ownerProjection('"department_id","company_id","person_id","version","updated_by","updated_at"','department_heads')).toBe(
      'SELECT "department_id","company_id","person_id","version","updated_by","updated_at" FROM department_heads');
    expect(ownerProjection('"id","engagement_id","previous_level_id","level_id","actor_id","reason","command_id","created_at","insertion_order"','staff_level_history')).toBe(
      'SELECT "id","engagement_id","previous_level_id","level_id","actor_id","reason","command_id","created_at","insertion_order" FROM staff_level_history');
    expect(ownerProjection('"id","company_id","code","name","rank","active","version","created_at","updated_at","level_number"','staff_levels')).toBe(
      'SELECT "id","company_id","code","name","rank","active","version","created_at","updated_at","level_number" FROM staff_levels');
    expect(ownerProjection('"company_id","version","exceptions_json","reason","updated_by","updated_at"','spending_approval_templates')).toBe(
      'SELECT "company_id","version","exceptions_json","reason","updated_by","updated_at" FROM spending_approval_templates');
    expect(ownerProjection('"id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision"','organization_nodes')).toBe(
      'SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes');
    expect(ownerProjection('"id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required"','organization_positions')).toBe(
      'SELECT "id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required" FROM organization_positions');
    expect(ownerProjection('"company_id","status","version"','company_business_acceptance')).toBe(
      'SELECT "company_id","status","version" FROM company_business_acceptance');
    expect(ownerProjection('id,company_id,branch_id,kind,status','organization_nodes')).toBe(
      'SELECT id,company_id,branch_id,kind,status FROM organization_nodes');
  });

  test('r35 defines one new helper and every r17-r34 helper is untouched', () => {
    expect(helper.match(/export const ownerProjection =/g) ?? []).toHaveLength(1);
    const prior = read('organization-column-alias-families.ts');
    const r27 = read('organization-column-alias-families-r27.ts');
    const r28 = read('organization-column-alias-families-r28.ts');
    const r29 = read('organization-column-alias-families-r29.ts');
    const r30 = read('organization-column-alias-families-r30.ts');
    const r31 = read('organization-column-alias-families-r31.ts');
    const r34 = read('organization-column-alias-families-r34.ts');
    const tail = read('organization-read-tail-sources.ts');
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
    for (const name of ['STAFF_LEVELS_L_JOIN', 'NODE_BRANCH_ACTIVE', 'departmentSnapshotMissing', 'ownerVersionGuardSource']) {
      expect(tail.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('remaining spaced JOIN constants stay byte-identical and are not merged', async () => {
    const spaced = await import('../../../packages/server-organization/src/adapters/organization-nodes-spaced-projections');
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'c')).toBe(spaced.ORG_NODES_SPACED_C_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'n')).toBe(spaced.ORG_NODES_SPACED_N_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'b')).toBe(spaced.ORG_NODES_SPACED_B_JOIN);
    expect(spaced.ORG_NODES_SPACED_C_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c');
  });

  test('inline bare-projection literals are gone and each source map delegates', async () => {
    for (const sql of [
      'SELECT "department_id","company_id","person_id","version","updated_by","updated_at" FROM department_heads',
      'SELECT "id","engagement_id","previous_level_id","level_id","actor_id","reason","command_id","created_at","insertion_order" FROM staff_level_history',
      'SELECT "id","company_id","code","name","rank","active","version","created_at","updated_at","level_number" FROM staff_levels',
      'SELECT "company_id","version","exceptions_json","reason","updated_by","updated_at" FROM spending_approval_templates',
      'SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes',
      'SELECT "id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required" FROM organization_positions',
      'SELECT "company_id","status","version" FROM company_business_acceptance',
      'SELECT id,company_id,branch_id,kind,status FROM organization_nodes',
    ]) {
      for (const src of [finalRoot, lastAudit, org, platform, talent]) expect(src).not.toContain(sql);
    }
    for (const src of [finalRoot, lastAudit, org, platform, talent]) expect(src.split('ownerProjection(').length - 1).toBeGreaterThanOrEqual(1);

    const { ORG_NODES_SOURCE, ORG_POSITIONS_SOURCE } = await import('../../../packages/server-organization/src/adapters/org-sources');
    const { finalRootOwnerProjectionSources } = await import('../../../packages/server-organization/src/adapters/final-root-owner-projection-sources');
    const { lastAuditOwnerProjectionSources } = await import('../../../packages/server-organization/src/adapters/last-audit-owner-projection-sources');
    const { platformOwnerProjectionSources } = await import('../../../packages/server-organization/src/adapters/platform-owner-projection-sources');
    const { talentSettlementPlacements } = await import('../../../packages/server-organization/src/adapters/talent-settlement-sources');
    expect(ORG_NODES_SOURCE).toBe(ownerProjection('"id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision"','organization_nodes'));
    expect(ORG_POSITIONS_SOURCE).toBe(ownerProjection('"id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required"','organization_positions'));
    expect(finalRootOwnerProjectionSources.department_heads).toBe(ownerProjection('"department_id","company_id","person_id","version","updated_by","updated_at"','department_heads'));
    expect(finalRootOwnerProjectionSources.staff_level_history).toBe(ownerProjection('"id","engagement_id","previous_level_id","level_id","actor_id","reason","command_id","created_at","insertion_order"','staff_level_history'));
    expect(finalRootOwnerProjectionSources.staff_levels).toBe(ownerProjection('"id","company_id","code","name","rank","active","version","created_at","updated_at","level_number"','staff_levels'));
    expect(lastAuditOwnerProjectionSources.spending_approval_templates).toBe(ownerProjection('"company_id","version","exceptions_json","reason","updated_by","updated_at"','spending_approval_templates'));
    expect(platformOwnerProjectionSources.company_business_acceptance).toBe(ownerProjection('"company_id","status","version"','company_business_acceptance'));
    expect(talentSettlementPlacements).toBe(ownerProjection('id,company_id,branch_id,kind,status','organization_nodes'));
  });

  test('locks and native-read-tail guarantees are retained', () => {
    const talent = read('talent-settlement-sources.ts');
    expect(talent).toContain('LOCK TABLE organization_nodes IN SHARE MODE');
    for (const src of [finalRoot, lastAudit, org, platform, talent]) expect(src).not.toContain('UPDATE ');
  });
});

describe('Organization read-tail r35 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r35 BEFORE=272 baseline', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r35 cut one new family (8 literals -> 1 helper statement) below the
    // task BEFORE=272; no other owner path is touched.
    expect(space).toBe(265);
    expect(space).toBeLessThan(272);
  });
});
