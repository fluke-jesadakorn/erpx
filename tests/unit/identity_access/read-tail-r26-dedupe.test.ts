import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { pmsRecipientProjectionSql } from '../../../packages/server-identity_access/src/adapters/pms-recipient-projection-family-r26';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const helperSrc = read('adapters/pms-recipient-projection-family-r26.ts');
const commandSrc = read('adapters/pms-command-facts.ts');
const recipientSrc = read('adapters/pms-source-recipients.ts');
const norm = (s: string) => s.replace(/\s+/g, ' ').trim();
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

// Both pre-dedupe literals shared this exact head; only the injected
// people-snapshot join fragment and the appended tail differed. Reconstruction
// here pins the rendered SQL, so a drift in projection/predicates fails.
const J = '⟪JOIN⟫';
const head = pmsRecipientProjectionSql(J);
const commandBefore = `SELECT DISTINCT a.user_id id,COALESCE(h.legal_name,a.user_id) name FROM assignments a LEFT ${J} ON h.user_id=a.user_id AND h.company_id=a.company_id AND h.merged_into_id IS NULL WHERE a.company_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?)) ORDER BY name`;
const commandAfter = `${head} AND h.merged_into_id IS NULL WHERE a.company_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?)) ORDER BY name`;
const recipientBefore = `SELECT DISTINCT a.user_id id,COALESCE(h.legal_name,a.user_id) name
    FROM assignments a LEFT ${J} ON h.user_id=a.user_id AND h.company_id=a.company_id
    AND h.status='active' AND h.merged_into_id IS NULL
    WHERE a.company_id=? AND a.user_id<>? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?
    AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
    AND a.scope_kind IN ('holding','branch') AND (?::text IS NULL OR a.scope_kind='holding' OR a.scope_node_id=?)
    AND iam_role_allows(a.role,a.company_id,'pms.read') ORDER BY name,a.user_id LIMIT 100`;
const recipientAfter = `${head}
    AND h.status='active' AND h.merged_into_id IS NULL
    WHERE a.company_id=? AND a.user_id<>? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?
    AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
    AND a.scope_kind IN ('holding','branch') AND (?::text IS NULL OR a.scope_kind='holding' OR a.scope_node_id=?)
    AND iam_role_allows(a.role,a.company_id,'pms.read') ORDER BY name,a.user_id LIMIT 100`;

describe('Identity access read-tail r26 PMS recipient projection family', () => {
  test('renders SQL identical to both pre-dedupe literals', () => {
    expect(head).toBe('SELECT DISTINCT a.user_id id,COALESCE(h.legal_name,a.user_id) name FROM assignments a LEFT ' + J + ' ON h.user_id=a.user_id AND h.company_id=a.company_id');
    expect(commandAfter).toBe(commandBefore); // byte-identical
    expect(norm(recipientAfter)).toBe(norm(recipientBefore)); // whitespace-only difference
    expect((head.match(/SELECT /g) ?? []).length).toBe(1);
  });

  test('is one owner helper with two call sites and no inline literal left behind', () => {
    expect((helperSrc.match(/export const pmsRecipientProjectionSql =/g) ?? []).length).toBe(1);
    expect((helperSrc.match(/\bpmsRecipientProjectionSql\b/g) ?? []).length).toBe(1); // definition only
    expect((commandSrc.match(/\bpmsRecipientProjectionSql\b/g) ?? []).length).toBe(2); // import + call
    expect((recipientSrc.match(/\bpmsRecipientProjectionSql\b/g) ?? []).length).toBe(2); // import + call
    const marker = 'FROM assignments a LEFT ${peoplepayrollSources.';
    expect((commandSrc.match(new RegExp(esc(marker), 'g')) ?? []).length).toBe(0);
    expect((recipientSrc.match(new RegExp(esc(marker), 'g')) ?? []).length).toBe(0);
  });

  test('introduces no write or lock and leaves locks/session authority/native kinds/earlier helpers untouched', () => {
    expect(helperSrc).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // lock families remain exactly where they were
    expect(read('adapters/payroll-cancellation-grant.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('SELECT * FROM sso_browser_transactions WHERE id=? FOR UPDATE');
    // session-authority families remain exactly where they were
    expect(read('adapters/iam-probe-sql.ts')).toContain('revokedSessionProbeSql');
    expect(read('adapters/iam-probe-sql.ts')).toContain('sessionByUserSql');
    expect(read('adapters/native-session.ts')).toContain('session');
    // earlier-round helpers still present exactly once
    const probeSql = read('adapters/iam-probe-sql.ts');
    for (const h of [
      'projectAuthorityProbeSql', 'roleAllowsSql', 'samePersonSql', 'activeRecipientsSql',
      'idNameByUserIdsSql', 'verifiedUserEmailSql', 'grantProbeSql', 'publishedRoleSql',
    ]) expect((probeSql.match(new RegExp(`export const ${h} =`, 'g')) ?? []).length, h).toBe(1);
    expect((read('adapters/identity-user-projection-family-r18.ts').match(/export const userAttributeByIdSql =/g) ?? []).length).toBe(1);
    expect((read('adapters/identity-user-projection-family-r21.ts').match(/export const userAttributeByIdExtendedSql =/g) ?? []).length).toBe(1);
    expect((read('adapters/active-role-probe-family-r25.ts').match(/export const activeIamRoleProbeSql =/g) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r26 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r26 BEFORE=517 baseline (expected 516)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(517);
    expect(total).toBeLessThanOrEqual(516);
  });
});
