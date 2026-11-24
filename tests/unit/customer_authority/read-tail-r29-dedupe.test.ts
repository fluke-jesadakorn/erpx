import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { reviewEligibilityRows } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r29-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Opaque, closed inputs reconstructed verbatim from the pre-dedupe call sites
// (r28 baseline, BEFORE=57). Only alias/table/columns/joins/manager are
// parameterized; the emitted text is pinned byte-for-byte.
const MGR_C = 'EXISTS(SELECT 1 FROM assignments c WHERE c.user_id=?)';
const MGR_R = 'EXISTS(SELECT 1 FROM assignments r WHERE r.user_id=?)';
const FIN = '(SELECT 1) finance_parties';

const ID_COLS = 'p.name customer_name,p.code customer_code,p.version current_customer_version';
const ID_JOIN = `JOIN (${FIN}) p ON p.id=c.customer_id AND p.company_id=c.company_id`;
const DUP_COLS = 'l.name left_name,l.code left_code,l.tax_registration left_tax,l.status left_status,l.merged_into_id left_merged_into_id,q.name right_name,q.code right_code,q.tax_registration right_tax,q.status right_status,q.merged_into_id right_merged_into_id';
const DUP_JOIN = `JOIN (${FIN}) l ON l.id=r.left_customer_id JOIN (${FIN}) q ON q.id=r.right_customer_id`;

const ID_ORIGINAL = `SELECT c.*,${ID_COLS},CASE WHEN ${MGR_C} THEN 1 ELSE 0 END actor_eligible FROM customer_identity_changes c ${ID_JOIN} WHERE c.company_id=? ORDER BY c.updated_at DESC,c.id`;
const DUP_ORIGINAL = `SELECT r.*,${DUP_COLS},CASE WHEN ${MGR_R} THEN 1 ELSE 0 END actor_eligible FROM customer_duplicate_reviews r ${DUP_JOIN} WHERE r.company_id=? ORDER BY r.updated_at DESC,r.id`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r29 review-eligibility row family', () => {
  test('renders byte-identically to both pre-dedupe literals', () => {
    expect(reviewEligibilityRows('c', 'customer_identity_changes', ID_COLS, ID_JOIN, MGR_C)).toBe(ID_ORIGINAL);
    expect(reviewEligibilityRows('r', 'customer_duplicate_reviews', DUP_COLS, DUP_JOIN, MGR_R)).toBe(DUP_ORIGINAL);
    readOnly(ID_ORIGINAL);
    readOnly(DUP_ORIGINAL);
  });

  test('one shared literal, no inline copies and pinned caller arguments', () => {
    const shared = src('customer-authority-read-tail-r29-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(shared).toContain('export const reviewEligibilityRows');
    expect(shared).toContain('CASE WHEN ${manager} THEN 1 ELSE 0 END actor_eligible');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const identity = src('identity.ts');
    expect(identity.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(identity).toContain("reviewEligibilityRows('c',");
    expect(identity).not.toContain(ID_ORIGINAL);

    const duplicates = src('duplicates.ts');
    expect(duplicates.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(duplicates).toContain("reviewEligibilityRows('r',");
    expect(duplicates).not.toContain(DUP_ORIGINAL);

    // Native read kinds and manager-eligibility helpers stay as the callers' arguments.
    expect(identity).toContain('CA_RD_IDENTITY_CHANGE_HISTORY,nativeOwnerReadAll');
    expect(duplicates).toContain('CA_RD_DUPLICATE_REVIEW_HISTORY,nativeOwnerReadAll');
    expect(identity).toContain("managerEligibilitySql('c')");
    expect(duplicates).toContain("managerEligibilitySql('r')");
  });

  test('r21-r28 builders, source locks and native kinds stay untouched', () => {
    const counts: Array<[string, number, string]> = [
      ['customer-authority-read-sql.ts', 2, 'export const jsonbAggFact'],
      ['package-obligation-sql.ts', 3, 'export const latestSupportVersion'],
      ['customer-authority-read-tail-r23-sources.ts', 1, 'export const jsonbImageProjection'],
      ['customer-authority-read-tail-r24-sources.ts', 1, 'export const bindingFactProjection'],
      ['customer-authority-read-tail-r25-sources.ts', 1, 'export const ownerColumnProjection'],
      ['customer-authority-read-tail-r26-sources.ts', 1, 'export const accessBindingOrderProjection'],
      ['customer-authority-read-tail-r27-sources.ts', 1, 'export const accessEventEvidenceProbe'],
      ['customer-authority-read-tail-r28-sources.ts', 1, 'export const releaseGateRead'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=57', () => {
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(srcRoot);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT/g) ?? []).length, 0);
    expect(total).toBeLessThan(57);
    expect(total).toBeLessThanOrEqual(52);
  });
});
