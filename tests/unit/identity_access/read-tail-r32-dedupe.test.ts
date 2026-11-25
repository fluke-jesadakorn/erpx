import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { assignmentCompanyUserProbeSql, iamMembersPairSql } from '../../../packages/server-identity_access/src/adapters/read-tail-family-r32';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Pre-dedupe literals captured from HEAD. Rendered helper output must match exactly.
const O_CAL = `SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND scope_kind='holding' AND iam_role_allows(role,company_id,'iam.directory.manage') AND iam_assignment_active(id,?) LIMIT 1`;
const O_COL = `SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL
      AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) LIMIT 1`;
const O_PLF = `?::text IS NULL OR EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND scope_kind='holding' AND iam_role_allows(role,company_id,'iam.directory.manage') AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`;
const O_IP = `SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND (login_policy IN ('emergency','directory') OR status!='active')`;
const O_IS = `SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND login_policy='emergency' AND status='active'`;
const TAIL_CAL = `scope_kind='holding' AND iam_role_allows(role,company_id,'iam.directory.manage') AND iam_assignment_active(id,?) LIMIT 1`;
const TAIL_COL = `revoked_at IS NULL
      AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) LIMIT 1`;
const TAIL_PLF = `scope_kind='holding' AND iam_role_allows(role,company_id,'iam.directory.manage') AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`;
const TAIL_IP = `(login_policy IN ('emergency','directory') OR status!='active')`;
const TAIL_IS = `login_policy='emergency' AND status='active'`;

describe('Identity access read-tail r32 byte oracles', () => {
  test('company+user assignment probe renders byte-identically at all three tails', () => {
    expect(assignmentCompanyUserProbeSql(TAIL_CAL)).toBe(O_CAL);
    expect(assignmentCompanyUserProbeSql(TAIL_COL)).toBe(O_COL);
    expect(`?::text IS NULL OR EXISTS(${assignmentCompanyUserProbeSql(TAIL_PLF)})`).toBe(O_PLF);
    expect(assignmentCompanyUserProbeSql('')).toBe('SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND ');
    expect(assignmentCompanyUserProbeSql(TAIL_CAL)).not.toBe(assignmentCompanyUserProbeSql(TAIL_COL));
  });

  test('iam_members pair probe renders byte-identically and keeps $n binds', () => {
    expect(iamMembersPairSql(TAIL_IP)).toBe(O_IP);
    expect(iamMembersPairSql(TAIL_IS)).toBe(O_IS);
    expect(iamMembersPairSql('')).toBe('SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND ');
    expect(iamMembersPairSql(TAIL_IP)).not.toBe(iamMembersPairSql(TAIL_IS));
    expect(iamMembersPairSql(TAIL_IP)).toContain('company_id=$1 AND user_id=$2');
  });
});

describe('Identity access read-tail r32 wiring', () => {
  test('one head per helper; all five call sites use them and keep tails and binds', () => {
    const helper = read('adapters/read-tail-family-r32.ts');
    expect((helper.match(/export const assignmentCompanyUserProbeSql =/g) ?? []).length).toBe(1);
    expect((helper.match(/export const iamMembersPairSql =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(2);

    expect(calls('adapters/calendar-commands-facts.ts', 'assignmentCompanyUserProbeSql')).toBe(2);
    expect(calls('adapters/collaboration-inbox-sources.ts', 'assignmentCompanyUserProbeSql')).toBe(2);
    expect(calls('adapters/people-lifecycle-facts.ts', 'assignmentCompanyUserProbeSql')).toBe(2);
    expect(calls('adapters/iam-probe-sql.ts', 'iamMembersPairSql')).toBe(2);
    expect(calls('adapters/iam-service.ts', 'iamMembersPairSql')).toBe(2);

    const cal = read('adapters/calendar-commands-facts.ts');
    const col = read('adapters/collaboration-inbox-sources.ts');
    const plf = read('adapters/people-lifecycle-facts.ts');
    const ip = read('adapters/iam-probe-sql.ts');
    const is = read('adapters/iam-service.ts');
    expect(cal).toContain(`assignmentCompanyUserProbeSql("${TAIL_CAL}")`);
    expect(cal).toContain('.bind(facts.value1, facts.value2, facts.value3)');
    expect(col).toContain('assignmentCompanyUserProbeSql(`revoked_at IS NULL');
    expect(col).toContain('      AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) LIMIT 1`)');
    expect(col).toContain('.bind(company_id,user_id,now,now2)');
    expect(plf).toContain(`"?::text IS NULL OR EXISTS(" + assignmentCompanyUserProbeSql("${TAIL_PLF}") + ")"`);
    expect(plf).toContain('[itAssigneeId, companyId, itAssigneeId2]');
    expect(ip).toContain(`protectedMigrationIdentitySql = iamMembersPairSql("${TAIL_IP}")`);
    expect(is).toContain(`c.query(iamMembersPairSql("${TAIL_IS}"), [actor.companyId, row.user_id])`);

    expect(cal).not.toContain('db.prepare(`SELECT 1 FROM assignments');
    expect(col).not.toContain('db.prepare(`SELECT 1 FROM assignments WHERE company_id=? AND user_id=?');
    expect(plf).not.toContain('OR EXISTS(SELECT 1 FROM assignments');
    expect(ip).not.toContain('"SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND (');
    expect(is).not.toContain('"SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND login_policy');
  });

  test('locks, advisory, native kinds and r1-r31 helpers stay untouched', () => {
    const helper = read('adapters/read-tail-family-r32.ts');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(hash('adapters/iam-probe-sql.ts')).toBe('e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101');
    expect(hash('adapters/read-tail-family-r31.ts')).toBe('83c107b5cd94bcc29a51b14777dfa01675ada8b1d059c43bbfe8bb009e8cb9c4');
    expect(hash('adapters/read-tail-family-r30.ts')).toBe('2cde8f94073ca753169c63c8024ea217282a953d2420a1ae6f60c056a11675a0');
    expect(hash('adapters/read-tail-family-r29.ts')).toBe('2c392291ac6bfa7558a022f750fba0742acfe7014702875d8836321063650775');
    expect(hash('adapters/read-tail-family-r28.ts')).toBe('6f5f736acb5cc6c796584ec86dfe52dcc40408ad0122be17bccfb8b2c7d1051d');
    expect(hash('adapters/read-tail-family-r27.ts')).toBe('f921a14d81bfc53158005b4fdc270deaa7580a276bb52d409a36a50995cbce76');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('SELECT * FROM sso_browser_transactions WHERE id=? FOR UPDATE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });
});

describe('Identity access read-tail r32 retirement check', () => {
  test('retirement:ts-statements is 389 after finance-refund-notice plan retirement', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(390);
    expect(total).toBe(389);
  });
});
