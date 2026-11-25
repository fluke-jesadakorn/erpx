import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { byKey } from '../../../packages/server-sales/src/adapters/by-key';
import { followupCustomerOrderRead } from '../../../packages/server-sales/src/adapters/followup-customer-order-read';
import { followupCustomerOrderTailRead } from '../../../packages/server-sales/src/adapters/followup-customer-order-tail-read';
import { pointRead } from '../../../packages/server-sales/src/adapters/point-read';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const orderRead = read('followup-customer-order-read.ts');
const tailRead = read('followup-customer-order-tail-read.ts');
const byKeySource = read('by-key.ts');
const pointReadSource = read('point-read.ts');
const followups = read('followups.ts');
const calendar = read('calendar-customer-followups-facts.ts');
const purchases = read('followup-purchases.ts');
const customers = read('customers.ts');

const sha = (value: string) => createHash('sha256').update(value).digest('hex');

// Pre-r31 head renders with owner-local tokens replaced by fixed sentinels.
// Only composition moved; every clause, alias, predicate, column and bind stays
// byte-identical under the shared generators.
const pre = {
  list: `SELECT c.*,OWNER,o.number order_number,o.branch_id,o.department_id,p.name customer_name,CASE WHEN ELIGIBLE THEN 1 ELSE 0 END owner_eligible FROM customer_followups c JOIN (ORDERS) o ON o.id=c.order_id AND o.company_id=c.company_id JOIN (PARTIES) p ON p.id=o.party_id AND p.company_id=o.company_id WHERE SCOPE AND PURCHASE AND (?::text IS NULL OR c.id=?) AND (?::text IS NULL OR c.order_id=?) ORDER BY c.due_date,c.id`,
  calendar: `SELECT c.*,o.branch_id,o.department_id,OWNER FROM customer_followups c JOIN (ORDERS) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE c.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND INLIST AND c.status='open' AND c.due_date>=? AND c.due_date<? AND (?='team' OR c.owner_id=? OR c.created_by=?) AND (?::text IS NULL OR c.owner_id IN(SELECT user_id FROM (HRPEOPLE) hr_people WHERE id=? AND company_id=c.company_id)) AND (?='' OR c.next_action ILIKE '%'||?||'%') AND PURCHASE ORDER BY c.due_date,c.id LIMIT ?`,
  work: `SELECT c.id,native.order_kind,BOOL AND ASSIGN AND c.status='open' AND c.owner_id=? AND ELIGIBLE can_act FROM customer_followups c JOIN (ORDERS) o ON o.id=c.order_id AND o.company_id=c.company_id JOIN (NATIVE) native ON native.id=c.id AND native.company_id=c.company_id WHERE c.company_id=? AND c.id IN (?,?) AND PURCHASE`,
  heads: `SELECT DISTINCT ON (o.party_id) o.party_id,c.id,c.due_date,c.next_action,c.order_id FROM customer_followups c JOIN (ORDERS) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE SCOPE AND PURCHASE AND o.party_id IN (?) AND c.status='open' ORDER BY o.party_id,c.due_date,c.id`,
  followups: `SELECT c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number FROM customer_followups c JOIN (ORDERS) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE SCOPE AND PURCHASE AND o.party_id=? AND c.status='open' ORDER BY c.due_date,c.id LIMIT 30`,
  byKeySettings: `SELECT * FROM commerce_customer_notification_settings WHERE company_id=?`,
  byKeyItems: `SELECT * FROM sales_price_list_items WHERE price_list_id=?`,
  point: `SELECT * FROM T WHERE C=?`,
  pointOrdered: `SELECT C1,C2 FROM T WHERE C=? ORDER BY X`,
} as const;

const preSha = {
  list: '9b8617a731cb2bbd7db79960a61aead2abf45acdb3bc99c4b6fc3582ac4ef743',
  calendar: 'ab34373e197b4cdf3b89d23d96c4ac4f27099807c73bb0f372f5a9be89298a22',
  work: '2bf29c329518e0a2451db7c27eabe144e1dc0bed3329d0bec34beb20137c3f9e',
  heads: 'e0ef94f9cdcbfc9db33f7f90e6c90c94c5450a15d964f7623bafc8839f9b11d1',
  followups: 'bd2d3db0275b7ab23c6076d0f2fc17912053483601b88eb27af4edbbb1d27106',
  byKeySettings: '8f3843b3d170743fc12de888c4e4845da326bcd815db695c6986f02481a53242',
  byKeyItems: '9b0969157e73761ab0092356994690afa36acf6e787ed9aa000c8217c8ccdf4a',
  point: '882bb7570e2e65994f1256d7b89d6c141f036ec857b0ddef642ae78246a8db8a',
  pointOrdered: '0fc32212b7318c03c8198163ba36e480af1dd58aa853e223132950cd9c3d80ea',
} as const;

describe('Sales read-tail r31 tail-parameterized followup order join', () => {
  const list = followupCustomerOrderTailRead(
    'ORDERS',
    'c.*,OWNER,o.number order_number,o.branch_id,o.department_id,p.name customer_name,CASE WHEN ELIGIBLE THEN 1 ELSE 0 END owner_eligible',
    'SCOPE',
    'PURCHASE',
    '(?::text IS NULL OR c.id=?) AND (?::text IS NULL OR c.order_id=?)',
    'ORDER BY c.due_date,c.id',
    ' JOIN (PARTIES) p ON p.id=o.party_id AND p.company_id=o.company_id',
  );
  const calendarHead = followupCustomerOrderTailRead(
    'ORDERS',
    'c.*,o.branch_id,o.department_id,OWNER',
    'c.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)',
    "INLIST AND c.status='open' AND c.due_date>=? AND c.due_date<? AND (?='team' OR c.owner_id=? OR c.created_by=?) AND (?::text IS NULL OR c.owner_id IN(SELECT user_id FROM (HRPEOPLE) hr_people WHERE id=? AND company_id=c.company_id)) AND (?='' OR c.next_action ILIKE '%'||?||'%')",
    'PURCHASE',
    'ORDER BY c.due_date,c.id LIMIT ?',
  );

  test.each([
    ['list', list, pre.list],
    ['calendar', calendarHead, pre.calendar],
  ] as ['list' | 'calendar', string, string][])(
    '%s head stays byte-identical to its pre-r31 literal',
    (key, actual, expected) => {
      expect(actual).toBe(expected);
      expect(sha(actual)).toBe(preSha[key]);
    },
  );

  test('one primary generator, tail delegator, no inlined head at either caller', () => {
    expect(typeof followupCustomerOrderTailRead).toBe('function');
    expect(followupCustomerOrderTailRead.length).toBe(6);
    expect((tailRead.match(/export const followupCustomerOrderTailRead\s*=/g) ?? []).length).toBe(1);
    expect((tailRead.match(/SELECT/g) ?? []).length).toBe(0);
    expect(tailRead).toContain("import {followupCustomerOrderRead} from './followup-customer-order-read';");
    expect(tailRead).toContain('followupCustomerOrderRead(orders, columns, scope, purchaseWhere, whereTail, order, joinTail)');
    expect(followups).toContain('followupCustomerOrderTailRead(');
    expect(calendar).toContain('followupCustomerOrderTailRead(');
    expect(followups).not.toContain('`SELECT c.*,${ownerNameProjection},o.number order_number');
    expect(calendar).not.toContain('`SELECT c.*,o.branch_id,o.department_id,${ownerNameProjection}');
    for (const source of [tailRead, orderRead, followups, calendar]) {
      expect(source).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });

  test('purchase.where, ownerNameProjection and both tails stay in the rendered head', () => {
    expect(followups).toContain('scope,purchase.where,');
    expect(calendar).toContain('`,purchase.where,\'ORDER BY c.due_date,c.id LIMIT ?\')');
    expect(followups).toContain('`c.*,${ownerNameProjection},o.number order_number');
    expect(calendar).toContain('`c.*,o.branch_id,o.department_id,${ownerNameProjection}`');
    expect(list).toContain('WHERE SCOPE AND PURCHASE AND (?::text IS NULL OR c.id=?)');
    expect(list.endsWith('ORDER BY c.due_date,c.id')).toBe(true);
    expect(calendarHead.endsWith('AND PURCHASE ORDER BY c.due_date,c.id LIMIT ?')).toBe(true);
    expect(followups).toContain('.bind(...args(a),...purchase.args,id??null,id??null,orderId??null,orderId??null)');
    expect(calendar).toContain('...purchase.args, facts.value11');
  });
});

describe('Sales read-tail r31 byKey single-key family', () => {
  const settings = byKey('commerce_customer_notification_settings', 'company_id');
  const items = byKey('sales_price_list_items', 'price_list_id');

  test.each([
    ['byKeySettings', settings, pre.byKeySettings],
    ['byKeyItems', items, pre.byKeyItems],
    ['point', pointRead('*', 'T', 'C'), pre.point],
    ['pointOrdered', pointRead('C1,C2', 'T', 'C', ' ORDER BY X'), pre.pointOrdered],
  ] as ('byKeySettings' | 'byKeyItems' | 'point' | 'pointOrdered')[][])(
    '%s stays byte-identical through pointRead',
    (key, actual, expected) => {
      expect(actual).toBe(expected);
      expect(sha(actual)).toBe(preSha[key]);
    },
  );

  test('byKey delegates to pointRead and pointRead stays the only literal', () => {
    expect(byKeySource).toContain('export const byKey =');
    expect(byKeySource).toContain("pointRead('*', table, column)");
    expect((byKeySource.match(/SELECT/g) ?? []).length).toBe(0);
    expect((pointReadSource.match(/SELECT/g) ?? []).length).toBe(0);
    expect((pointReadSource.match(/export const pointRead\s*=/g) ?? []).length).toBe(1);
    expect(followups).toContain("pointRead('*','customer_followup_events','followup_id',' ORDER BY version DESC')");
  });
});

describe('Sales read-tail r31 followup work-projection head', () => {
  const work = followupCustomerOrderRead(
    'ORDERS',
    "c.id,native.order_kind,BOOL AND ASSIGN AND c.status='open' AND c.owner_id=? AND ELIGIBLE can_act",
    'c.company_id=? AND c.id IN (?,?)',
    'PURCHASE',
    '',
    '',
    ' JOIN (NATIVE) native ON native.id=c.id AND native.company_id=c.company_id',
  );

  test('work head stays byte-identical with empty predicate and order', () => {
    expect(work).toBe(pre.work);
    expect(sha(work)).toBe(preSha.work);
  });

  test.each([
    ['heads', followupCustomerOrderRead('ORDERS', 'DISTINCT ON (o.party_id) o.party_id,c.id,c.due_date,c.next_action,c.order_id', 'SCOPE', 'PURCHASE', "o.party_id IN (?) AND c.status='open'", 'ORDER BY o.party_id,c.due_date,c.id'), pre.heads],
    ['followups', followupCustomerOrderRead('ORDERS', 'c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number', 'SCOPE', 'PURCHASE', "o.party_id=? AND c.status='open'", 'ORDER BY c.due_date,c.id LIMIT 30'), pre.followups],
  ] as ['heads' | 'followups', string, string][])(
    'customers %s still renders byte-identically',
    (key, actual, expected) => {
      expect(actual).toBe(expected);
      expect(sha(actual)).toBe(preSha[key]);
    },
  );

  test('one primary generator with default-empty extras and unchanged binds', () => {
    expect((orderRead.match(/export const followupCustomerOrderRead\s*=/g) ?? []).length).toBe(1);
    expect((orderRead.match(/SELECT/g) ?? []).length).toBe(1);
    expect(orderRead).toContain("predicate = '',");
    expect(orderRead).toContain("order = '',");
    expect(orderRead).toContain("extraJoin = '',");
    expect(purchases).toContain("projection.where,'','',` JOIN (${followupPurchaseRows}) native ON native.id=c.id AND native.company_id=c.company_id`");
    expect(purchases).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.userId,actor.companyId,...ids,...projection.args)');
    expect(customers).toContain("followupCustomerOrderRead(commerceProjectionSources.commerce_orders,'DISTINCT ON (o.party_id)");
    expect(customers).toContain("followupCustomerOrderRead(commerceProjectionSources.commerce_orders,'c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number'");
    expect(purchases).not.toContain('`SELECT c.id,native.order_kind');
  });
});

describe('Sales read-tail r31 retirement check', () => {
  test('LOCK TABLE, authorityScopeRead and native reads stay untouched', () => {
    const source = read('followup-source-sql.ts');
    const obligations = read('package-obligations.ts');
    const pricingPurchases = read('pricing-purchases.ts');
    const native = read('native-sales-read.ts');
    const rustRegistry = readFileSync(resolve(srcRoot, 'adapters/native_sales_read_sql.rs'), 'utf8');
    expect(source).toContain('export const authorityScopeRead=');
    expect(source).toContain('export const clockNowMs=');
    expect(source).toContain('export const followupEvidencePrefix=');
    expect(source).toContain('export const followupMetricPrefix=');
    expect(source).toContain("export const actorScopeTuple=scopeTuple('actor');");
    expect(pricingPurchases).toContain('authorityScopeRead(');
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(pricingPurchases).toContain('LOCK TABLE sales_price_lists,sales_price_list_items IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ACTIVE');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ITEMS_ACTIVE');
    expect(native).toContain('export const SALES_RD_NOTIFICATION_ATTEMPT');
    expect(native).toContain('export const SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED');
    expect((rustRegistry.match(/pub const RD_\w+: &str =/g) ?? []).length).toBe(4);
  });

  test('retirement:ts-statements drops from 50 to 45', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r31: tail variant (+1), list (-1), calendar (-1), work (-1), byKey (-1).
    // r33: tail generator delegates to the primary generator (-1).
    // r34: pointRead delegates to the shared tableRead generator (-1).
    expect(total).toBe(45);
    expect(total).toBeLessThan(50);
  });
});
