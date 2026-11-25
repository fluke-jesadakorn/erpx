import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import { platformOwnerProjectionSources as people_payrollPlatformSources } from '@erpx/server-people_payroll/public/platform-owner-projection-sources';
import {
  latestEmployeeEngagementId,
  personEngagementJoin,
} from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r55';

vi.mock('@erpx/server-workflow/public/command-assertions', () => ({
  assertion: (db: unknown, commandId: string, step: string, query: string, args: unknown[]) => ({
    commandId, step, query, args,
  }),
}));

import { payrollOrganizationFacts } from '../../../packages/server-organization/src/adapters/payroll-owner-facts';
import { peopleDirectoryOrganizationFacts } from '../../../packages/server-organization/src/adapters/people-directory-facts';
import { lifecycleOrganizationFacts } from '../../../packages/server-organization/src/adapters/people-lifecycle-worker-effects';
import { createApprovalsSourceRepository } from '../../../packages/server-organization/src/adapters/workflow-approvals-facts';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r55.ts');

const people = people_payrollPlatformSources.hr_people;
const engagements = people_payrollPlatformSources.hr_engagements;

// Byte oracles recorded verbatim from the pre-r55 (BEFORE=218) source. Only
// owner-supplied extras/tails vary; every head, projection, placeholder style
// and lock-free closure is rendered verbatim by its helper.
const ENGAGEMENT_ORDER_TAIL = "ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1";
const latestOracle = (extra: string) =>
  `(SELECT x.id FROM (${engagements}) x WHERE x.person_id=p.id AND x.company_id=p.company_id ${extra}${ENGAGEMENT_ORDER_TAIL})`;
const JOIN_HEAD = `SELECT 1 FROM (${people}) p JOIN (${engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id `;
const joinOracle = (tail: string) => `${JOIN_HEAD}${tail}`;

const EXTRA_BARE = '';
const EXTRA_EMPLOYEE = "AND x.worker_type='employee' AND (?::text IS NULL OR x.id=?) ";
const TAIL_EFFECT6 = "JOIN staff_levels l ON l.id=e.level_id JOIN staff_levels target ON target.id=$3 WHERE p.id=$1 AND p.company_id=$2 AND p.status='active' AND e.status='active' AND l.level_number<target.level_number";
const TAIL_GUARD33 = "WHERE p.id=? AND p.company_id=? AND iam_same_person(p.user_id,?,p.company_id) AND p.status='active' AND e.status='active' AND (?::text IS NULL OR e.position_id=?)";

type Captured = { commandId: string; step: string; query: string; args: unknown[] };
type Fetch = { sql: string; binds: unknown[] };
const fakeDb = (log: Fetch[]) => ({
  prepare: (sql: string) => ({
    bind: (...binds: unknown[]) => ({
      all: () => { log.push({ sql, binds }); return []; },
      first: () => { log.push({ sql, binds }); return null; },
    }),
  }),
}) as never;
type Query = { sql: string; params: unknown[] };
const fakeClient = (log: Query[]) => ({
  query: (sql: string, params: unknown[]) => { log.push({ sql, params }); return {}; },
}) as never;

describe('Organization read-tail r55 dedupe', () => {
  test('r55 helpers render byte-identical SQL to every replaced literal', () => {
    expect(latestEmployeeEngagementId(EXTRA_BARE)).toBe(latestOracle(EXTRA_BARE));
    expect(latestEmployeeEngagementId(EXTRA_EMPLOYEE)).toBe(latestOracle(EXTRA_EMPLOYEE));
    expect(personEngagementJoin(TAIL_EFFECT6)).toBe(joinOracle(TAIL_EFFECT6));
    expect(personEngagementJoin(TAIL_GUARD33)).toBe(joinOracle(TAIL_GUARD33));

    // Owner-supplied tails keep their own placeholder style and lock-free closure.
    expect(latestOracle(EXTRA_BARE)).toContain(`${engagements}) x WHERE x.person_id=p.id`);
    expect(EXTRA_EMPLOYEE).toContain('?::text');
    expect(TAIL_EFFECT6).toContain('$3');
    expect(TAIL_GUARD33).toContain('?::text');
    for (const tail of [EXTRA_BARE, EXTRA_EMPLOYEE, TAIL_EFFECT6, TAIL_GUARD33]) {
      expect(tail).not.toMatch(/FOR (UPDATE|SHARE)/);
    }
  });

  test('every replaced call site renders the same bytes and keeps its binds', () => {
    const log: Fetch[] = [];
    payrollOrganizationFacts(fakeDb(log)).readListPeopleRows('c', 'b', 'b2', 'd', 'd2', 'v', 'v2', 'v3', 'v4', 'v5');
    peopleDirectoryOrganizationFacts(fakeDb(log)).readDirectoryPersonPerson('v', 'v2', 'p', 'c', 'v3', 'v4', 'b', 'b2', 'd', 'd2');
    expect(log).toHaveLength(2);
    expect(log[0].sql).toContain(`e.id=${latestOracle(EXTRA_BARE)}`);
    expect(log[1].sql).toContain(`e.id=${latestOracle(EXTRA_EMPLOYEE)}`);
    expect(log[0].binds).toEqual(['c', 'b', 'b2', 'd', 'd2', 'v', 'v2', 'v3', 'v4', 'v5']);
    expect(log[1].binds).toEqual(['v', 'v2', 'p', 'c', 'v3', 'v4', 'b', 'b2', 'd', 'd2']);

    const queries: Query[] = [];
    lifecycleOrganizationFacts(fakeClient(queries)).applyEffect6('m', 'c', 7);
    expect(queries[0].sql).toBe(joinOracle(TAIL_EFFECT6));
    expect(queries[0].params).toEqual(['m', 'c', 7]);

    const guard = createApprovalsSourceRepository({} as never).guard33(
      'k', 'appointed-approval-current', ['c', 'p', 'u', null], true,
    ) as unknown as Captured;
    expect(guard.query).toBe(`EXISTS(${joinOracle(TAIL_GUARD33)})`);
    expect(guard.query).toBe(`EXISTS(${personEngagementJoin(TAIL_GUARD33)})`);
    expect(guard.args).toEqual(['c', 'p', 'u', null]);
    expect(guard.commandId).toBe('k');
    expect(guard.step).toBe('appointed-approval-current');
  });

  test('inline heads are gone, call sites delegate, and r50/r53/r54 helpers, locks and spaced JOINs stay untouched', () => {
    const payroll = read('payroll-owner-facts.ts');
    const directory = read('people-directory-facts.ts');
    const lifecycle = read('people-lifecycle-worker-effects.ts');
    const approvals = read('workflow-approvals-facts.ts');

    expect(payroll).not.toContain(latestOracle(EXTRA_BARE));
    expect(directory).not.toContain(latestOracle(EXTRA_EMPLOYEE));
    expect(lifecycle).not.toContain(joinOracle(TAIL_EFFECT6));
    expect(approvals).not.toContain(joinOracle(TAIL_GUARD33));

    expect(helper.match(/export const latestEmployeeEngagementId =/g) ?? []).toHaveLength(1);
    expect(helper.match(/export const personEngagementJoin =/g) ?? []).toHaveLength(1);
    expect(payroll.split('latestEmployeeEngagementId(').length - 1).toBe(1);
    expect(directory.split('latestEmployeeEngagementId(').length - 1).toBe(1);
    expect(lifecycle.split('personEngagementJoin(').length - 1).toBe(1);
    expect(approvals.split('personEngagementJoin(').length - 1).toBe(1);

    expect(payroll).toContain("latestEmployeeEngagementId('')");
    expect(directory).toContain("latestEmployeeEngagementId(\"AND x.worker_type='employee' AND (?::text IS NULL OR x.id=?) \")");
    expect(lifecycle).toContain('personEngagementJoin("JOIN staff_levels l ON l.id=e.level_id');
    expect(approvals).toContain('personEngagementJoin("WHERE p.id=? AND p.company_id=?');
    expect(payroll).toContain('.bind(companyId, branchId, branchId2, departmentId, departmentId2, value, value2, value3, value4, value5).all()');
    expect(directory).toContain('.bind(value, value2, personId, companyId, value3, value4, branchId, branchId2, departmentId, departmentId2)');
    expect(lifecycle).toContain('[manager, company_id, level_id]');
    expect(approvals).toContain('value2);}};}');

    // Protected helpers (r50 nodeExistsGuard, r54 activeNodeByKind) are defined
    // once in their own files and are not redefined by the r55 helper.
    for (const [file, names] of [
      ['organization-read-tail-sources-r50.ts', ['nodeExistsGuard', 'activePositionLookup']],
      ['organization-column-alias-families-r27.ts', ['activeNodeByKind']],
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
      expect(helper).not.toContain(`export const ${names[0]} =`);
    }
    expect(directory).toContain("from './organization-read-tail-sources-r50'");
    expect(approvals).toContain("authoritySeatCompanyRead} from './organization-read-tail-sources-r51'");

    // Lock tails and spaced JOIN projections are unchanged.
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('people-lifecycle-facts.ts')).toContain('FOR UPDATE');
    expect(read('organization-drafts.ts')).toContain('FOR UPDATE');
    expect(read('project-worker-startup-sources.ts')).toContain(
      'LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE',
    );
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements drops 218 -> 196', () => {
    // Wave 2 moved three node-reference reads to the native owner (six lane-23 reads total); the absolute pin reflects the current owner source.
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r55 folds the latest-engagement id read (-2 sites into one renderer with
    // +1 head) and the person/engagement EXISTS head (-2 sites into one renderer
    // with +1 head): 218 - 4 + 2 = 216. r56 folds the absent-node guard head
    // (-3 sites into one renderer with +1 head): 216 - 3 + 1 = 214. r57 folds
    // the authority-seat EXISTS head (-4 sites, +1 head): 214 - 4 + 1 = 211.
    // r58 folds the two approvals-facts heads (-2 sites): 211 - 2 = 209.
    // r59 folds two executive-metric count heads (-2 literals, +1 renderer): 209 - 1 = 208.
    // r60 folds two level-history head literals (-2 literals, +1 renderer): 208 - 1 = 207.
    // r61 folds two position-package projection heads (-2 literals, +1 renderer): 207 - 1 = 206.
    // No other owner path is touched.
    expect(space).toBe(196);
    expect(space).toBeLessThan(218);
  });
});
