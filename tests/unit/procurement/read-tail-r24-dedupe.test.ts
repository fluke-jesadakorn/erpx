import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { eligiblePurchaseCount, memberScopeExclusion } from '../../../packages/server-procurement/src/adapters/read-tail-r24';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);
const M = 'MEMBERS', A = 'ASSIGN';

describe('Procurement read-tail r24 parameterized family dedupe', () => {
  test('memberScopeExclusion renders each original iam_members exclusion byte-identically', () => {
    expect(memberScopeExclusion(M, 'owner', 'r.company_id', 'candidate.user_id', ' IN(r.created_by,r.submitted_by,o.recorded_by)')).toBe(
      `NOT EXISTS(SELECT 1 FROM (${M}) cm JOIN (${M}) owner ON owner.company_id=cm.company_id AND owner.person_id=cm.person_id WHERE cm.company_id=r.company_id AND cm.user_id=candidate.user_id AND owner.user_id IN(r.created_by,r.submitted_by,o.recorded_by))`,
    );
    expect(memberScopeExclusion(M, 'creator', 'c.company_id', 'c.user_id', '=?')).toBe(
      `NOT EXISTS(SELECT 1 FROM (${M}) cm JOIN (${M}) creator ON creator.company_id=cm.company_id AND creator.person_id=cm.person_id WHERE cm.company_id=c.company_id AND cm.user_id=c.user_id AND creator.user_id=?)`,
    );
  });

  test('eligiblePurchaseCount renders both original purchase-eligibility counts byte-identically', () => {
    expect(eligiblePurchaseCount(A, 'a', 'o.company_id', 'o.branch_id', 'o.department_id')).toBe(
      `(SELECT count(DISTINCT a.user_id) FROM (${A}) a WHERE a.company_id=o.company_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.purchase') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)))`,
    );
    const tail = ` AND (r.status<>'review' OR (iam_role_allows(candidate.role,candidate.company_id,'commerce.approve') AND candidate.user_id<>r.created_by AND candidate.user_id<>COALESCE(r.submitted_by,'') AND candidate.user_id<>COALESCE(o.recorded_by,'') AND ${memberScopeExclusion(M, 'owner', 'r.company_id', 'candidate.user_id', ' IN(r.created_by,r.submitted_by,o.recorded_by)')}))`;
    expect(eligiblePurchaseCount(A, 'candidate', 'r.company_id', 'r.branch_id', 'r.department_id', tail)).toBe(
      `(SELECT count(DISTINCT candidate.user_id) FROM (${A}) candidate WHERE candidate.company_id=r.company_id AND iam_assignment_active(candidate.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(candidate.role,candidate.company_id,'commerce.purchase') AND (candidate.scope_kind='holding' OR (candidate.scope_kind='branch' AND candidate.scope_node_id=r.branch_id) OR (candidate.scope_kind='department' AND candidate.scope_node_id=r.department_id)) AND (r.status<>'review' OR (iam_role_allows(candidate.role,candidate.company_id,'commerce.approve') AND candidate.user_id<>r.created_by AND candidate.user_id<>COALESCE(r.submitted_by,'') AND candidate.user_id<>COALESCE(o.recorded_by,'') AND NOT EXISTS(SELECT 1 FROM (${M}) cm JOIN (${M}) owner ON owner.company_id=cm.company_id AND owner.person_id=cm.person_id WHERE cm.company_id=r.company_id AND cm.user_id=candidate.user_id AND owner.user_id IN(r.created_by,r.submitted_by,o.recorded_by)))))`,
    );
  });

  test('callers use the helpers and keep no inline family fragments', () => {
    expect(src('supplier-promises.ts')).toContain('eligiblePurchaseCount(identity_accessPlatformSources.assignments,"a","o.company_id","o.branch_id","o.department_id")');
    expect(src('rfqs.ts')).toContain('eligiblePurchaseCount(identity_accessPlatformSources.assignments,"candidate"');
    expect(src('rfqs.ts')).toContain('memberScopeExclusion(identity_accessPlatformSources.iam_members,"owner","r.company_id","candidate.user_id"');
    expect(src('supplier-bills.ts')).toContain('memberScopeExclusion(identity_accessPlatformSources.iam_members,"creator","c.company_id","c.user_id","=?")');
    expect(src('supplier-promises.ts')).not.toContain('(SELECT count(DISTINCT a.user_id) FROM (${identity_accessPlatformSources.assignments})');
    expect(src('rfqs.ts')).not.toContain('(SELECT count(DISTINCT candidate.user_id) FROM (${identity_accessPlatformSources.assignments})');
    expect(src('rfqs.ts')).not.toContain('NOT EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.iam_members}) cm JOIN');
    expect(src('supplier-bills.ts')).not.toContain('NOT EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.iam_members}) cm JOIN');
  });

  test('no write, lock, command_receipts or r17-r23 helper was touched', () => {
    const tail = src('read-tail-sql.ts');
    expect(tail).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(tail).toContain('export const pointRead =');
    expect(tail).toContain('export const orderedRead =');
    expect(src('read-tail-r19.ts')).toContain('export const tableRead =');
    expect(src('read-tail-r20.ts')).toContain('export const receiptCount =');
    expect(src('read-tail-r21.ts')).toContain('export const jsonbAggBy =');
    expect(src('read-tail-r22.ts')).toContain('export const jsonbAggJoinBy =');
    expect(src('read-tail-r23.ts')).toContain('export const returnedWithoutReplacement =');
    expect(src('read-tail-r23.ts')).toContain('export const billReceiptBinding =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=142', () => {
    const root = new URL('../../../packages/server-procurement/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(142);
  });
});
