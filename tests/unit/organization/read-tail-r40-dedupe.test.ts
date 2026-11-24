import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  activePositionList,
  recommendedPayDefault,
  nodeChangeByIdCompany,
  activeStaffLevels,
  memberProfileByUser,
} from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r40';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r40.ts');

// Replaced literals, recorded verbatim from the pre-r40 (BEFORE=257) source.
const POSITION_PREFIX = ' FROM organization_positions WHERE company_id=? AND active=1 ';
const POSITIONS = [
  `SELECT id,title,level_id${POSITION_PREFIX}ORDER BY title`,
  `SELECT id,title,branch_id,department_id,level_id,version${POSITION_PREFIX}ORDER BY id`,
  `SELECT id,title,branch_id,department_id,level_id,is_ceo${POSITION_PREFIX}ORDER BY title,id`,
];
const POSITION_ARGS: [string, string][] = [
  ['id,title,level_id', 'ORDER BY title'],
  ['id,title,branch_id,department_id,level_id,version', 'ORDER BY id'],
  ['id,title,branch_id,department_id,level_id,is_ceo', 'ORDER BY title,id'],
];
const PAY_SUFFIX = ' FROM position_pay_defaults d JOIN organization_positions p ON p.id=d.position_id WHERE d.company_id=? AND d.position_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?) ORDER BY d.version DESC LIMIT 1';
const PAY = [`SELECT d.*${PAY_SUFFIX}`, `SELECT d.currency,d.monthly_amount_minor${PAY_SUFFIX}`];
const PAY_ARGS = ['d.*', 'd.currency,d.monthly_amount_minor'];
const CHANGE_PREFIX = 'SELECT ';
const CHANGE_SUFFIX = ' FROM organization_node_changes WHERE id=? AND company_id=?';
const CHANGES = [
  `${CHANGE_PREFIX}*${CHANGE_SUFFIX}`,
  `${CHANGE_PREFIX}company_id,NULL::text branch_id,NULL::text department_id,version,requested_by,state${CHANGE_SUFFIX}`,
];
const CHANGE_ARGS = ['*', 'company_id,NULL::text branch_id,NULL::text department_id,version,requested_by,state'];
const LEVELS = [
  'SELECT id,name FROM staff_levels WHERE company_id=? AND active=1 ORDER BY rank',
  'SELECT id,name FROM staff_levels WHERE company_id=? AND active=1 ORDER BY rank,id',
];
const LEVEL_ARGS = ['ORDER BY rank', 'ORDER BY rank,id'];
const PROFILE_SUFFIX = ' FROM member_profiles WHERE user_id=?';
const PROFILES = [
  `SELECT user_id,bio,links_json,avatar_key,cover_key,avatar_media_type,cover_media_type,version${PROFILE_SUFFIX}`,
  `SELECT \u00ABkind\u00BB_key AS key,\u00ABkind\u00BB_media_type AS media_type${PROFILE_SUFFIX}`,
];
const PROFILE_ARGS = [
  'user_id,bio,links_json,avatar_key,cover_key,avatar_media_type,cover_media_type,version',
  '\u00ABkind\u00BB_key AS key,\u00ABkind\u00BB_media_type AS media_type',
];

describe('Organization read-tail r40 byte-identical families', () => {
  test('r40 helpers render byte-identical SQL to every replaced literal', () => {
    POSITION_ARGS.forEach(([columns, orderBy], i) => expect(activePositionList(columns, orderBy)).toBe(POSITIONS[i]));
    PAY_ARGS.forEach((projection, i) => expect(recommendedPayDefault(projection)).toBe(PAY[i]));
    CHANGE_ARGS.forEach((projection, i) => expect(nodeChangeByIdCompany(projection)).toBe(CHANGES[i]));
    LEVEL_ARGS.forEach((orderBy, i) => expect(activeStaffLevels(orderBy)).toBe(LEVELS[i]));
    PROFILE_ARGS.forEach((projection, i) => expect(memberProfileByUser(projection)).toBe(PROFILES[i]));
  });

  test('r40 defines five new helpers and every r17-r39 helper is untouched', () => {
    for (const name of ['activePositionList', 'recommendedPayDefault', 'nodeChangeByIdCompany', 'activeStaffLevels', 'memberProfileByUser']) {
      expect(helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    const prior = read('organization-column-alias-families.ts');
    const r39 = read('organization-read-tail-sources-r39.ts');
    for (const name of ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']) {
      expect(prior.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['companyNodeScopeSource', 'activeStaffLevelSource', 'changeVersionGuardSource', 'scopeNodeBranchExists']) {
      expect(r39.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline family literals are gone, call sites delegate, locks and spaced JOIN constants are retained', () => {
    const peopleEval = read('people-evaluation-facts.ts');
    const gpo = read('gpo-sources.ts');
    const pws = read('project-worker-startup-sources.ts');
    const staffDefaults = read('staff-defaults.ts');
    const lifecycle = read('people-lifecycle-facts.ts');
    const org = read('organization.ts');
    const finance = read('attachments-finance-facts.ts');
    const member = read('member-profiles.ts');

    for (const p of POSITIONS) expect(`${peopleEval}${gpo}${pws}`).not.toContain(p);
    for (const p of PAY) expect(`${staffDefaults}${lifecycle}`).not.toContain(p);
    for (const p of CHANGES) expect(`${org}${finance}`).not.toContain(p);
    for (const p of LEVELS) expect(`${peopleEval}${pws}`).not.toContain(p);
    for (const p of PROFILES) expect(member).not.toContain(p);

    expect(peopleEval.split('activePositionList(').length - 1).toBe(1);
    expect(gpo.split('activePositionList(').length - 1).toBe(1);
    expect(pws.split('activePositionList(').length - 1).toBe(1);
    expect(peopleEval.split('activeStaffLevels(').length - 1).toBe(1);
    expect(pws.split('activeStaffLevels(').length - 1).toBe(1);
    expect(staffDefaults.split('recommendedPayDefault(').length - 1).toBe(1);
    expect(lifecycle.split('recommendedPayDefault(').length - 1).toBe(1);
    expect(org.split('nodeChangeByIdCompany(').length - 1).toBe(1);
    expect(finance.split('nodeChangeByIdCompany(').length - 1).toBe(1);
    expect(member.split('memberProfileByUser(').length - 1).toBe(2);

    // FOR SHARE/FOR UPDATE locks and spaced JOIN constants stay byte-identical.
    expect(read('pms-command-facts.ts')).toContain("FOR SHARE");
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r40 BEFORE=257 baseline (expected 251)', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r40 collapses five families (-2, -1, -1, -1, -1 = -6) into shared sources.
    expect(space).toBe(234);
    expect(space).toBeLessThan(257);
  });
});
