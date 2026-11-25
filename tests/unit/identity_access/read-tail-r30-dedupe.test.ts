import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { identitySnapshotSources as peoplepayrollSnapshotSources } from '@erpx/server-people_payroll/public/identity-snapshot-sources';
import {
  assignmentCompanyProbeSql, payrollOwnerVersionProbeSql, umGroupProbeSql, protectedPersonOwnerSql,
} from '../../../packages/server-identity_access/src/adapters/read-tail-family-r30';

const peoplepayrollSources = peoplepayrollSnapshotSources();
const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Pre-dedupe literals captured from HEAD. Every rendered helper output must match exactly.
const O_F4_AGENT = `EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?))`;
const O_F4_MESSAGING = `EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL AND valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`;
const O_F4_APPROVALS = `EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND id=? AND iam_role_allows(role,company_id,'workflow.'||?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))`;
const O_F4_LIFECYCLE = `EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (?::text IS NULL OR iam_role_allows(role,company_id,?)) AND (? NOT IN ('payroll','finance','directory') OR scope_kind='holding'))`;
const O_F5_MAPPING = `EXISTS(SELECT 1 ${peoplepayrollSources.payroll_owner_facts_hr_payroll_account_mappings_from} WHERE id=? AND company_id=? AND status='draft' AND version=? AND NOT iam_same_person(created_by,?,company_id))`;
const O_F5_RUN = `EXISTS(SELECT 1 ${peoplepayrollSources.payroll_owner_facts_hr_payroll_runs_from} WHERE id=? AND company_id=? AND version=? AND status='in_review' AND submitted_by IS NOT NULL AND NOT iam_same_person(submitted_by,?,company_id) AND submitted_digest=input_digest AND input_digest IS NOT NULL)`;
const O_F5_BATCH = `EXISTS(SELECT 1 ${peoplepayrollSources.payroll_owner_facts_hr_payment_batches_from} WHERE id=? AND company_id=? AND version=? AND status='draft' AND NOT iam_same_person(COALESCE(retry_requested_by,created_by),?,company_id))`;
const O_F6_TECHNOLOGY = `SELECT 1 FROM um_groups WHERE company_id=$1 AND id=$2 AND NOT (definition->>'archived')::boolean AND um_person_in_unit($1,$3,definition->>'scopeNodeId')`;
const O_F6_LIFECYCLE = `SELECT 1 FROM um_groups WHERE company_id=$1 AND id=$2 AND NOT (definition->>'archived')::boolean`;
const O_F7_OWNER = `SELECT 1 FROM assignments a LEFT JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id WHERE a.company_id=? AND people_person_user(?,?,a.user_id) AND a.revoked_at IS NULL AND (a.role='holding_owner' OR m.login_policy='emergency')`;
const O_F7_GUARD = `NOT people_person_user(?,?,?) AND NOT EXISTS(${O_F7_OWNER})`;

describe('Identity access read-tail r30 probe families', () => {
  test('all 11 rendered statements are byte-identical to their pre-dedupe literals', () => {
    expect(`EXISTS(${assignmentCompanyProbeSql("user_id=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)")})`).toBe(O_F4_AGENT);
    expect(`EXISTS(${assignmentCompanyProbeSql("user_id=? AND revoked_at IS NULL AND valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint")})`).toBe(O_F4_MESSAGING);
    expect(`EXISTS(${assignmentCompanyProbeSql("id=? AND iam_role_allows(role,company_id,'workflow.'||?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?))")})`).toBe(O_F4_APPROVALS);
    expect(`EXISTS(${assignmentCompanyProbeSql("user_id=? AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (?::text IS NULL OR iam_role_allows(role,company_id,?)) AND (? NOT IN ('payroll','finance','directory') OR scope_kind='holding')")})`).toBe(O_F4_LIFECYCLE);
    expect(`EXISTS(${payrollOwnerVersionProbeSql(peoplepayrollSources.payroll_owner_facts_hr_payroll_account_mappings_from, "status='draft' AND version=? AND NOT iam_same_person(created_by,?,company_id)")})`).toBe(O_F5_MAPPING);
    expect(`EXISTS(${payrollOwnerVersionProbeSql(peoplepayrollSources.payroll_owner_facts_hr_payroll_runs_from, "version=? AND status='in_review' AND submitted_by IS NOT NULL AND NOT iam_same_person(submitted_by,?,company_id) AND submitted_digest=input_digest AND input_digest IS NOT NULL")})`).toBe(O_F5_RUN);
    expect(`EXISTS(${payrollOwnerVersionProbeSql(peoplepayrollSources.payroll_owner_facts_hr_payment_batches_from, "version=? AND status='draft' AND NOT iam_same_person(COALESCE(retry_requested_by,created_by),?,company_id)")})`).toBe(O_F5_BATCH);
    expect(umGroupProbeSql(" AND um_person_in_unit($1,$3,definition->>'scopeNodeId')")).toBe(O_F6_TECHNOLOGY);
    expect(umGroupProbeSql('')).toBe(O_F6_LIFECYCLE);
    expect(protectedPersonOwnerSql).toBe(O_F7_OWNER);
    expect(`NOT people_person_user(?,?,?) AND NOT EXISTS(${protectedPersonOwnerSql})`).toBe(O_F7_GUARD);
  });

  test('keeps the four heads read-only (no writes or row locks)', () => {
    const all = [
      assignmentCompanyProbeSql('user_id=?'),
      payrollOwnerVersionProbeSql('hr_payroll_runs', "version=?"),
      umGroupProbeSql(''),
      protectedPersonOwnerSql,
    ].join(' ');
    expect(all).toContain('SELECT 1 FROM assignments WHERE company_id=? AND ');
    expect(all).toContain('SELECT 1 FROM um_groups WHERE company_id=$1 AND id=$2');
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('one definition per head; every call site uses it and no inline head remains', () => {
    const helper = read('adapters/read-tail-family-r30.ts');
    for (const name of ['assignmentCompanyProbeSql', 'payrollOwnerVersionProbeSql', 'umGroupProbeSql', 'protectedPersonOwnerSql']) {
      expect((helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).length, name).toBe(1);
    }
    expect((helper.match(/SELECT /g) ?? []).length).toBe(4);
    // Counts include the import binding plus each call site (r13-r29 convention).
    const counts: [string, string, number][] = [
      ['adapters/agent-governance-facts.ts', 'assignmentCompanyProbeSql', 2],
      ['adapters/collaboration-messaging-authorization.ts', 'assignmentCompanyProbeSql', 2],
      ['adapters/workflow-approvals-facts.ts', 'assignmentCompanyProbeSql', 2],
      ['adapters/people-lifecycle-facts.ts', 'assignmentCompanyProbeSql', 2],
      ['adapters/people-lifecycle-facts.ts', 'umGroupProbeSql', 2],
      ['adapters/people-lifecycle-facts.ts', 'protectedPersonOwnerSql', 3],
      ['adapters/technology.ts', 'umGroupProbeSql', 2],
      ['adapters/payroll-owner-facts.ts', 'payrollOwnerVersionProbeSql', 4],
    ];
    for (const [rel, name, n] of counts) expect(calls(rel, name), `${rel} ${name}`).toBe(n);
    for (const [rel, inline] of [
      ['adapters/agent-governance-facts.ts', 'SELECT 1 FROM assignments WHERE company_id=? AND '],
      ['adapters/collaboration-messaging-authorization.ts', 'SELECT 1 FROM assignments WHERE company_id=? AND '],
      ['adapters/workflow-approvals-facts.ts', 'SELECT 1 FROM assignments WHERE company_id=? AND '],
      ['adapters/people-lifecycle-facts.ts', "EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,floor("],
      ['adapters/technology.ts', "SELECT 1 FROM um_groups WHERE company_id=$1 AND id=$2"],
      ['adapters/people-lifecycle-facts.ts', "SELECT 1 FROM um_groups WHERE company_id=$1 AND id=$2"],
      ['adapters/people-lifecycle-facts.ts', "WHERE a.company_id=? AND people_person_user(?,?,a.user_id) AND a.revoked_at IS NULL"],
      ['adapters/payroll-owner-facts.ts', 'EXISTS(SELECT 1 ${peoplepayrollSources.payroll_owner_facts'],
    ] as [string, string][]) {
      expect(read(rel).includes(inline), `${rel} still inlines ${inline}`).toBe(false);
    }
  });

  test('locks, session authority, native kinds and r1-r29 helpers stay untouched', () => {
    const helper = read('adapters/read-tail-family-r30.ts');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(hash('adapters/product-sso-store.ts')).toBe('967b754535bb65e0196bf47fa0c0292c65b30276291e137c926d22579a3f3441');
    expect(hash('adapters/workflow-command-authority.ts')).toBe('d0939ee195594843eb159709f7214996667b7174d418235750e4ca09c93268ca');
    expect(hash('adapters/payroll-cancellation-grant.ts')).toBe('eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee');
    expect(hash('adapters/legacy-module-access.ts')).toBe('b90209698ed4adfbd08ae9de45e5619e2f5a9ebffdf4c92a10a065dc4bc5a01e');
    expect(hash('adapters/iam-transactions.ts')).toBe('ba2ca1aa3d1a6ea08f20c1647e5ff0bd506e667f14b6ccd07409eb06db3174e3');
    expect(hash('adapters/native-session.ts')).toBe('1c390e8f6c47b25da718120ef5f6546103b1f8bbbfa837406ff7be462d1a51c1');
    expect(hash('adapters/iam-probe-sql.ts')).toBe('e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101');
    expect(hash('adapters/active-role-probe-family-r25.ts')).toBe('aea7ee49d4c93ac3c904b5f8190119a277ad43921a21a70903cfdf5c0455d448');
    expect(hash('adapters/read-tail-family-r27.ts')).toBe('f921a14d81bfc53158005b4fdc270deaa7580a276bb52d409a36a50995cbce76');
    expect(hash('adapters/read-tail-family-r28.ts')).toBe('6f5f736acb5cc6c796584ec86dfe52dcc40408ad0122be17bccfb8b2c7d1051d');
    expect(hash('adapters/read-tail-family-r29.ts')).toBe('2c392291ac6bfa7558a022f750fba0742acfe7014702875d8836321063650775');
    expect(hash('adapters/identity-user-projection-family-r18.ts')).toBe('55d639d10814e811b947cb23a30bf27055f5e150552f516365f36418568937a9');
    expect(hash('adapters/identity-user-projection-family-r21.ts')).toBe('cf0fdc9322d730d211a206f0a07e661c081743887d445d5b0ddc97ff41be0b04');
    expect(hash('adapters/pms-recipient-projection-family-r26.ts')).toBe('60917ed0d495413680ce96a5d97dcfff89ef31eaa6d8bef480c0e65161f1e7c4');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('SELECT * FROM sso_browser_transactions WHERE id=? FOR UPDATE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });
});

describe('Identity access finance-refund-notice native read plan', () => {
  const FRAGMENT =
    'JOIN (SELECT "id","user_id","role","modules_json","company_id","branch_id","valid_from","valid_until","revoked_at","version","scope_kind","scope_node_id","parent_id","source","source_id" FROM assignments) a';

  test('the dead TypeScript producer and its re-export are gone', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/finance-refund-notice-source.ts'))).toBe(false);
    expect(existsSync(resolve(srcRoot, 'public/finance-refund-notice-source.ts'))).toBe(false);
  });

  test('the owner Rust plan pins the retired fragment byte for byte', () => {
    const plan = read('adapters/native_read_plan.rs');
    // The Rust literal escapes its double quotes; the rendered bytes are unescaped.
    expect(plan).toContain(FRAGMENT.replace(/"/g, '\\"'));
    expect(plan).toContain('dc11981c00603befbc6a70880dede5a69363220831ddb77a70c9800ce0edf44f');
    expect(createHash('sha256').update(FRAGMENT).digest('hex')).toBe(
      'dc11981c00603befbc6a70880dede5a69363220831ddb77a70c9800ce0edf44f',
    );
    const declaration = plan.split('\n').find((line) => line.startsWith('    "JOIN (SELECT')) ?? '';
    expect(declaration).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE/i);
    expect(read('public/mod.rs')).toContain('native_read_plan');
  });
});

describe('Identity access read-tail r30 retirement check', () => {
  test('retirement:ts-statements is 368 after access-diagnosis plan cut', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(369);
    expect(total).toBe(368);
  });
});
