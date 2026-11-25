import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  eligiblePurchaseCount,
  memberScopeExclusion,
} from '../../../packages/server-procurement/src/adapters/read-tail-r24';
import { expenseAccountBy } from '../../../packages/server-procurement/src/adapters/read-tail-r20';
import { settingsRead } from '../../../packages/server-procurement/src/adapters/read-tail-r27';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);
const M = 'MEMBERS', A = 'ASSIGN';

describe('Procurement read-tail r43 parameterized family dedupe', () => {
  test('eligiblePurchaseCount keeps the default render and the routed finance approver count byte-identical', () => {
    // r24 default: supplier-promises order + rfqs workRows (unchanged).
    expect(eligiblePurchaseCount(A, 'a', 'o.company_id', 'o.branch_id', 'o.department_id')).toBe(
      `(SELECT count(DISTINCT a.user_id) FROM (${A}) a WHERE a.company_id=o.company_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.purchase') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)))`,
    );
    // supplier-bills.ts:149 finance.write + finance.approve count.
    expect(eligiblePurchaseCount(A, 'c', '?', '?', '?',
      ` AND c.user_id<>? AND ${memberScopeExclusion(M, 'creator', 'c.company_id', 'c.user_id', '=?')}`,
      {
        roles: `iam_role_allows(c.role,c.company_id,'finance.write') AND iam_role_allows(c.role,c.company_id,'finance.approve')`,
        wrapper: b => b,
        output: ' n',
      },
    )).toBe(
      `SELECT count(DISTINCT c.user_id) n FROM (${A}) c WHERE c.company_id=? AND iam_assignment_active(c.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(c.role,c.company_id,'finance.write') AND iam_role_allows(c.role,c.company_id,'finance.approve') AND (c.scope_kind='holding' OR (c.scope_kind='branch' AND c.scope_node_id=?) OR (c.scope_kind='department' AND c.scope_node_id=?)) AND c.user_id<>? AND ${memberScopeExclusion(M, 'creator', 'c.company_id', 'c.user_id', '=?')}`,
    );
  });

  test('expenseAccountBy keeps both id-guarded renders and the routed expense account list byte-identical', () => {
    const base = `FROM (${A}) a JOIN (CHARTS) c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.chart_id=? AND a.currency=c.base_currency AND a.category='expense' AND a.status='active'`;
    expect(expenseAccountBy(A, 'CHARTS', 'a.id')).toBe(`SELECT a.id ${base}`);
    expect(expenseAccountBy(A, 'CHARTS', '1')).toBe(`SELECT 1 ${base}`);
    // supplier-bills.ts:114 expense account list: no id guard, trailing ORDER BY.
    expect(expenseAccountBy(A, 'CHARTS', 'a.id,a.code,a.name', { idGuard: '', order: ' ORDER BY a.code' })).toBe(
      `SELECT a.id,a.code,a.name FROM (${A}) a JOIN (CHARTS) c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.company_id=? AND a.chart_id=? AND a.currency=c.base_currency AND a.category='expense' AND a.status='active' ORDER BY a.code`,
    );
  });

  test('settingsRead keeps default renders and the routed rfq currency guard byte-identical', () => {
    expect(settingsRead('currency', 'SETTINGS')).toBe(
      'SELECT currency FROM (SETTINGS) commerce_settings WHERE company_id=?',
    );
    expect(settingsRead('*', 'SETTINGS')).toBe(
      'SELECT * FROM (SETTINGS) commerce_settings WHERE company_id=?',
    );
    expect(`EXISTS(${settingsRead('1', 'SETTINGS', ' AND currency=?')})`).toBe(
      'EXISTS(SELECT 1 FROM (SETTINGS) commerce_settings WHERE company_id=? AND currency=?)',
    );
  });

  test('callers use the parameterized helpers and keep no inline family fragments', () => {
    const bills = src('supplier-bills.ts');
    expect(bills).toContain("import {eligiblePurchaseCount,memberScopeExclusion} from './read-tail-r24';");
    expect(bills).toContain('eligiblePurchaseCount(identity_accessPlatformSources.assignments,"c","?","?","?"');
    expect(bills).toContain("iam_role_allows(c.role,c.company_id,'finance.write') AND iam_role_allows(c.role,c.company_id,'finance.approve')");
    expect(bills).toContain('expenseAccountBy(financePlatformSources.finance_accounts,financePlatformSources.finance_charts,"a.id,a.code,a.name",{idGuard:"",order:" ORDER BY a.code"})');
    expect(bills).not.toContain('SELECT count(DISTINCT c.user_id) n FROM (${identity_accessPlatformSources.assignments})');
    expect(bills).not.toContain('SELECT a.id,a.code,a.name FROM (${financePlatformSources.finance_accounts})');
    expect(bills).toContain("settingsRead(\"*\",commercePlatformSources.commerce_settings)");
    expect(bills).toContain(".bind(actor.companyId,row.branch_id,row.department_id,row.created_by,row.created_by).first('n')");
    expect(bills).toContain('.bind(actor.companyId, bill.chart_id).all<Row>()');
    const rfqs = src('rfqs.ts');
    expect(rfqs).toContain('`EXISTS(${settingsRead("1",commercePlatformSources.commerce_settings," AND currency=?")})`');
    expect(rfqs).not.toContain('EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_settings}) commerce_settings WHERE company_id=? AND currency=?)');
    expect(rfqs).toContain('const currencyRead=settingsRead("currency",commercePlatformSources.commerce_settings);');
  });

  test('no write, lock, command_receipts or r17-r41 helper export was touched', () => {
    const tail = src('read-tail-sql.ts');
    expect(tail).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(tail).toContain('export const pointRead =');
    expect(tail).toContain('export const orderedRead =');
    for (const [n, e] of [
      ['r19', 'tableRead'], ['r20', 'receiptCount'], ['r21', 'jsonbAggBy'], ['r22', 'jsonbAggJoinBy'],
      ['r23', 'billReceiptBinding'], ['r24', 'eligiblePurchaseCount'], ['r25', 'foreignBillReceiptWitness'],
      ['r26', 'fulfillmentLinesBy'], ['r27', 'settingsRead'], ['r28', 'attachmentVersionWitness'],
      ['r29', 'policyVersionGuard'], ['r30', 'orderScopedAggregate'], ['r31', 'sourceImage'],
      ['r32', 'progressOrderAggregate'], ['r33', 'quotedProjection'], ['r34', 'calendarScopedListRead'],
      ['r35', 'quotedFromSource'], ['r38', 'lineSourceWitness'], ['r39', 'actualSourceExists'],
      ['r40', 'billMatchProjection'], ['r41', 'openPromiseByOrder'],
    ] as const) expect(src(`read-tail-${n}.ts`)).toContain(`export const ${e} =`);
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=106', () => {
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
    expect(total).toBeLessThan(106);
    expect(total).toBe(90); // r44: 103 -> 99; r45: 99 -> 95; r47: six settlement fact heads (-6 +1 helper) 95 -> 90.
  });
});
