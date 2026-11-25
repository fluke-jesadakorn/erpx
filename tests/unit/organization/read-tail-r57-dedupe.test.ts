import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { authoritySeatExistsSql } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r57';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

const HEAD = 'EXISTS(SELECT 1 FROM authority_seats ';
const IMPORT_DEPT = "import { authoritySeatExistsSql } from './organization-read-tail-sources-r57';";
const IMPORT_ORG = "import {authoritySeatExistsSql} from './organization-read-tail-sources-r57';";

type Case = { label: string; tail: string; sha: string; binds: number; negated: boolean };

// Byte oracles recorded verbatim from the pre-r57 (BEFORE=214) source. Only the
// predicate tail after `authority_seats ` varies; the head and closing paren
// are rendered byte-identically by the helper and all bind arrays stay at the
// call sites. `negated` records where the caller supplies the `NOT ` prefix.
const CASES: Case[] = [
  {
    label: 'appointment-version absent',
    tail: 'WHERE company_id=? AND kind=? AND scope_id=?',
    sha: '2b4d825906856536426b4b174100fe200eac2fb6b73bbe339c8b1764581106d0',
    binds: 3,
    negated: true,
  },
  {
    label: 'appointment-version current',
    tail: 'WHERE company_id=? AND kind=? AND scope_id=? AND version=?',
    sha: '210412bbf34eb9ff373d8b52a25857456dfdc11c453ba4a15539595fbead5c96',
    binds: 4,
    negated: false,
  },
  {
    label: 'relocation head current',
    tail: "WHERE company_id=? AND kind='head' AND scope_id=? AND version=? AND position_id IS NOT DISTINCT FROM ? AND person_id IS NOT DISTINCT FROM ? AND effective_at IS NOT DISTINCT FROM ?",
    sha: '9943f1aa81a9b5c67879b6a003b7a8caa6d7af0da1e453c2dc0341ee1a8e1ddb',
    binds: 6,
    negated: false,
  },
  {
    label: 'relocation head absent',
    tail: "WHERE company_id=? AND kind='head' AND scope_id=?",
    sha: '461a34c47f2b2d9538d593455f6c5de981bc1d818e744e42faae5462a60243c0',
    binds: 2,
    negated: true,
  },
];

// Reconstructed independently of the helper body: the closed byte oracle.
const oracle = (tail: string) => `${HEAD}${tail})`;

describe('Organization read-tail r57 dedupe', () => {
  test('r57 helper renders byte-identically to every retired literal', () => {
    expect(CASES).toHaveLength(4);
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
    expect(CASES.filter((c) => c.negated)).toHaveLength(2);
    expect(CASES[0].tail).not.toBe(CASES[1].tail);
    expect(CASES[2].tail).not.toBe(CASES[3].tail);
    expect(authoritySeatExistsSql(CASES[0].tail)).not.toBe(authoritySeatExistsSql(CASES[1].tail));
  });

  test('all four call sites delegate with unchanged tails, binds and lock-free heads', () => {
    const dept = read('department-authority.ts');
    const org = read('organization.ts');

    expect(dept).toContain(IMPORT_DEPT);
    expect(org).toContain(IMPORT_ORG);
    expect(dept).toContain(
      "`(?=0 AND NOT ${authoritySeatExistsSql('WHERE company_id=? AND kind=? AND scope_id=?')}) OR ${authoritySeatExistsSql('WHERE company_id=? AND kind=? AND scope_id=? AND version=?')}`",
    );
    expect(org).toContain(
      '? authoritySeatExistsSql("WHERE company_id=? AND kind=\'head\' AND scope_id=? AND version=? AND position_id IS NOT DISTINCT FROM ? AND person_id IS NOT DISTINCT FROM ? AND effective_at IS NOT DISTINCT FROM ?")',
    );
    expect(org).toContain(': `NOT ${authoritySeatExistsSql("WHERE company_id=? AND kind=\'head\' AND scope_id=?")}`');

    const retired = CASES.map((c) => oracle(c.tail));
    for (const literal of retired) {
      expect(dept).not.toContain(literal);
      expect(org).not.toContain(literal);
    }
    expect(count(dept, 'authoritySeatExistsSql(')).toBe(2);
    expect(count(org, 'authoritySeatExistsSql(')).toBe(2);

    // Assertion bind arrays remain exactly where they were.
    expect(dept).toContain('[input.expectedVersion,actor.companyId,input.kind,scopeId,actor.companyId,input.kind,scopeId,input.expectedVersion]');
    expect(org).toContain('head?[actor.companyId,change.target_node_id,head.version,head.position_id,head.person_id,head.effective_at]:[actor.companyId,change.target_node_id]');

    // r58 completes the fold: the approvals-facts authority-seat heads now
    // delegate to the same r57 renderer; the r51/r55 helpers stay at the site.
    const approvals = read('workflow-approvals-facts.ts');
    expect(count(approvals, 'EXISTS(SELECT 1 FROM authority_seats ')).toBe(0);
    expect(approvals).toContain("import {authoritySeatCompanyRead} from './organization-read-tail-sources-r51';");
    expect(approvals).toContain("import {personEngagementJoin} from './organization-read-tail-sources-r55';");
    expect(count(approvals, 'authoritySeatExistsSql(')).toBe(2);
  });

  test('r48/r51/r50/r55/r56 helpers, locks and spaced joins stay untouched', () => {
    const r57 = read('organization-read-tail-sources-r57.ts');
    expect(count(r57, 'export const authoritySeatExistsSql =')).toBe(1);
    expect(count(r57, 'SELECT ')).toBe(1);
    for (const name of ['authoritySeatHolderGuard', 'authoritySeatCompanyRead', 'nodeExistsGuard', 'latestEmployeeEngagementId', 'notExistsOrgNodeSql']) {
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

    // Lock tails and spaced-join projections are unchanged.
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('people-lifecycle-facts.ts')).toContain('FOR UPDATE');
    expect(read('organization-drafts.ts')).toContain('FOR UPDATE');
    expect(read('project-worker-startup-sources.ts')).toContain(
      'LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE',
    );
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');

    for (const src of [read('department-authority.ts'), read('organization.ts')]) expect(src).not.toContain('FOR UPDATE');
  });

  test('retirement:ts-statements pins 214 -> 211', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r57 folds four authority-seat EXISTS HEAD literals into one renderer
    // (-4 literals, +1 declaration): 214 - 4 + 1 = 211. r58 folds the two
    // approvals-facts heads (-2 literals, no new declaration): 211 - 2 = 209.
    expect(CASES).toHaveLength(4);
    expect(total).toBe(209);
    expect(total).toBe(214 - 5);
    expect(total).toBeLessThan(214);
  });
});
