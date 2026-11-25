import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import { platformOwnerProjectionSources as people_payrollPlatformSources } from '@erpx/server-people_payroll/public/platform-owner-projection-sources';
import { authoritySeatExistsSql } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r57';
import { personEngagementJoin } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r55';

vi.mock('@erpx/server-workflow/public/command-assertions', () => ({
  assertion: (db: unknown, commandId: string, step: string, query: string, args: unknown[]) => ({
    commandId, step, query, args,
  }),
}));

import { createApprovalsSourceRepository } from '../../../packages/server-organization/src/adapters/workflow-approvals-facts';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

const people = people_payrollPlatformSources.hr_people;
const HEAD = 'EXISTS(SELECT 1 FROM authority_seats ';
const PLACEHOLDER = '${people_payrollPlatformSources.hr_people}';
const oracle = (tail: string) => `${HEAD}${tail})`;
const runtimeTail = (tail: string) => tail.replace(PLACEHOLDER, people);

// Byte oracles recorded verbatim from the pre-r58 (BEFORE=211) approvals
// source. Only the predicate tail after `authority_seats ` varies; the render
// head and closing paren come from the r57 helper and bind arrays stay at the
// call sites. `${...}` is opaque source text in these oracles, never expanded.
type Case = { label: string; tail: string; sha: string; binds: number };
const CASES: Case[] = [
  {
    label: 'guard12 hr seat (seat/person)',
    tail: "seat JOIN (${people_payrollPlatformSources.hr_people}) person ON person.id=seat.person_id AND person.company_id=seat.company_id WHERE seat.company_id=? AND seat.kind='hr' AND seat.scope_id=seat.company_id AND seat.person_id=? AND iam_same_person(person.user_id,?,person.company_id) AND person.status='active' AND seat.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
    sha: 'e80e67f4d644ee65a127eaef4d9b2ad810c85d859d6950145c6849a0c5996503',
    binds: 3,
  },
  {
    label: 'guard33 hr seat (s/p)',
    tail: "s JOIN (${people_payrollPlatformSources.hr_people}) p ON p.id=s.person_id AND p.company_id=s.company_id WHERE s.company_id=? AND s.kind='hr' AND s.scope_id=s.company_id AND s.person_id=? AND iam_same_person(p.user_id,?,p.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
    sha: '73f083062d09f9186dd9cb3a2f600dccb9d00836d12c0dec31efb1051b52c5f7',
    binds: 3,
  },
];

// The two retired literals, written out independently of the helper with the
// real owner projection substituted for interpolation.
const EXPECTED12 = `EXISTS(SELECT 1 FROM authority_seats seat JOIN (${people}) person ON person.id=seat.person_id AND person.company_id=seat.company_id WHERE seat.company_id=? AND seat.kind='hr' AND seat.scope_id=seat.company_id AND seat.person_id=? AND iam_same_person(person.user_id,?,person.company_id) AND person.status='active' AND seat.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`;
const EXPECTED33 = `EXISTS(SELECT 1 FROM authority_seats s JOIN (${people}) p ON p.id=s.person_id AND p.company_id=s.company_id WHERE s.company_id=? AND s.kind='hr' AND s.scope_id=s.company_id AND s.person_id=? AND iam_same_person(p.user_id,?,p.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`;

type Captured = { commandId: string; step: string; query: string; args: unknown[] };

describe('Organization read-tail r58 dedupe', () => {
  test('r57 renderer renders byte-identically to both retired tails', () => {
    expect(CASES).toHaveLength(2);
    for (const c of CASES) {
      const got = authoritySeatExistsSql(c.tail);
      expect(got).toBe(oracle(c.tail));
      expect(sha(got)).toBe(c.sha);
      expect(count(got, 'SELECT ')).toBe(1);
      expect((got.match(/\?/g) ?? []).length).toBe(c.binds);
      expect(got.startsWith(HEAD)).toBe(true);
      expect(got.endsWith(')')).toBe(true);
      expect(got).not.toMatch(/FOR (UPDATE|SHARE)/);
    }
    expect(CASES[0].tail).not.toBe(CASES[1].tail);
    expect(authoritySeatExistsSql(CASES[0].tail)).not.toBe(authoritySeatExistsSql(CASES[1].tail));
  });

  test('runtime SQL matches the pre-r58 literals byte for byte and keeps binds', () => {
    const repo = createApprovalsSourceRepository({} as never);
    const g12 = repo.guard12('k12', 'v12', 'c', 'p', 'u') as unknown as Captured;
    expect(g12.query).toBe(EXPECTED12);
    expect(g12.query).toBe(oracle(runtimeTail(CASES[0].tail)));
    expect(g12.args).toEqual(['c', 'p', 'u']);
    expect(g12.commandId).toBe('k12');
    expect(g12.step).toBe('v12');

    const g33 = repo.guard33('k33', 'appointed-approval-current', ['c', 'p', 'u', null], false) as unknown as Captured;
    expect(g33.query).toBe(EXPECTED33);
    expect(g33.query).toBe(oracle(runtimeTail(CASES[1].tail)));
    expect(g33.args).toEqual(['c', 'p', 'u', null]);

    // The policy-step branch still uses the r55 person/engagement head.
    const step = repo.guard33('k33b', 'appointed-approval-current', ['c', 'p', 'u', null], true) as unknown as Captured;
    expect(step.query).toBe(`EXISTS(${personEngagementJoin("WHERE p.id=? AND p.company_id=? AND iam_same_person(p.user_id,?,p.company_id) AND p.status='active' AND e.status='active' AND (?::text IS NULL OR e.position_id=?)")})`);
    expect(step.args).toEqual(['c', 'p', 'u', null]);
  });

  test('both sites delegate with unchanged tails; retired heads are gone', () => {
    const approvals = read('workflow-approvals-facts.ts');
    expect(count(approvals, "import {authoritySeatExistsSql} from './organization-read-tail-sources-r57';")).toBe(1);
    expect(count(approvals, 'authoritySeatExistsSql(')).toBe(2);
    expect(count(approvals, 'EXISTS(SELECT 1 FROM authority_seats ')).toBe(0);
    for (const c of CASES) {
      expect(approvals).toContain('authoritySeatExistsSql(`' + c.tail + '`)');
      expect(approvals).not.toContain(oracle(c.tail));
    }
    expect(approvals).toContain("import {authoritySeatCompanyRead} from './organization-read-tail-sources-r51';");
    expect(approvals).toContain("import {personEngagementJoin} from './organization-read-tail-sources-r55';");
    expect(approvals).toContain('[company_id,appointedPersonId,userId]');
    expect(approvals).toContain('value2);}};}');
    expect(approvals).not.toMatch(/FOR (UPDATE|SHARE)/);
    expect(sha(approvals)).toBe('c35091e4e00ce0ab13bfa08a5ec9d029f92f688656725b61f5ce8532d6f398f4');
  });

  test('r48 holder guard, other helpers, locks and spaced joins stay untouched', () => {
    const r57 = read('organization-read-tail-sources-r57.ts');
    expect(count(r57, 'export const authoritySeatExistsSql =')).toBe(1);
    expect(count(r57, 'SELECT ')).toBe(1);
    for (const name of [
      'authoritySeatHolderGuard', 'authoritySeatCompanyRead', 'nodeExistsGuard',
      'latestEmployeeEngagementId', 'personEngagementJoin', 'notExistsOrgNodeSql',
    ]) {
      expect(r57).not.toContain(`export const ${name} =`);
    }
    for (const [file, names] of [
      ['organization-read-tail-sources-r48.ts', ['authoritySeatHolderGuard']],
      ['organization-read-tail-sources-r51.ts', ['authoritySeatCompanyRead', 'activeDepartmentNode', 'memberProfileVersionGuard']],
      ['organization-read-tail-sources-r50.ts', ['nodeExistsGuard', 'activePositionLookup']],
      ['organization-read-tail-sources-r55.ts', ['latestEmployeeEngagementId', 'personEngagementJoin']],
      ['organization-read-tail-sources-r56.ts', ['notExistsOrgNodeSql']],
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(count(source, `export const ${name} =`)).toBe(1);
    }
    expect(read('organization-read-tail-sources-r48.ts')).toContain(HEAD);
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('people-lifecycle-facts.ts')).toContain('FOR UPDATE');
    expect(read('organization-drafts.ts')).toContain('FOR UPDATE');
    expect(read('project-worker-startup-sources.ts')).toContain(
      'LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE',
    );
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements pins 211 -> 196', () => {
    // Wave 2 moved three node-reference reads to the native owner (six lane-23 reads total); the absolute pin reflects the current owner source.
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r57 folds four authority-seat EXISTS heads into one renderer: 214 - 3 = 211.
    // r58 folds the two approvals-facts heads (-2 literals, no new declaration).
    // r59 folds two executive-metric count heads (-2 literals, +1 renderer): 209 - 1 = 208.
    // r60 folds two level-history head literals (-2 literals, +1 renderer): 208 - 1 = 207.
    // r61 folds two position-package projection heads (-2 literals, +1 renderer): 207 - 1 = 206.
    expect(CASES).toHaveLength(2);
    expect(total).toBe(196);
    expect(total).toBe(196);
    expect(total).toBeLessThan(211);
  });
});
