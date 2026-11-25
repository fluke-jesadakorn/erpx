import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { followupCustomerOrderRead } from '../../../packages/server-sales/src/adapters/followup-customer-order-read';
import { followupCustomerOrderTailRead } from '../../../packages/server-sales/src/adapters/followup-customer-order-tail-read';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const orderRead = read('followup-customer-order-read.ts');
const tailRead = read('followup-customer-order-tail-read.ts');
const followups = read('followups.ts');
const calendar = read('calendar-customer-followups-facts.ts');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

// Byte oracles captured at the pre-r33 head with owner-local tokens replaced by
// fixed sentinels. The tail generator must render these exactly after folding.
const oracle = {
  list: {
    sql: `SELECT c.*,OWNER,o.number order_number,o.branch_id,o.department_id,p.name customer_name,CASE WHEN ELIGIBLE THEN 1 ELSE 0 END owner_eligible FROM customer_followups c JOIN (ORDERS) o ON o.id=c.order_id AND o.company_id=c.company_id JOIN (PARTIES) p ON p.id=o.party_id AND p.company_id=o.company_id WHERE SCOPE AND PURCHASE AND (?::text IS NULL OR c.id=?) AND (?::text IS NULL OR c.order_id=?) ORDER BY c.due_date,c.id`,
    sha: '9b8617a731cb2bbd7db79960a61aead2abf45acdb3bc99c4b6fc3582ac4ef743',
  },
  calendar: {
    sql: `SELECT c.*,o.branch_id,o.department_id,OWNER FROM customer_followups c JOIN (ORDERS) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE c.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND INLIST AND c.status='open' AND c.due_date>=? AND c.due_date<? AND (?='team' OR c.owner_id=? OR c.created_by=?) AND (?::text IS NULL OR c.owner_id IN(SELECT user_id FROM (HRPEOPLE) hr_people WHERE id=? AND company_id=c.company_id)) AND (?='' OR c.next_action ILIKE '%'||?||'%') AND PURCHASE ORDER BY c.due_date,c.id LIMIT ?`,
    sha: 'ab34373e197b4cdf3b89d23d96c4ac4f27099807c73bb0f372f5a9be89298a22',
  },
} as const;

type Args = [string, string, string, string, string, string, string];
const cases: ['list' | 'calendar', Args][] = [
  ['list', [
    'ORDERS',
    'c.*,OWNER,o.number order_number,o.branch_id,o.department_id,p.name customer_name,CASE WHEN ELIGIBLE THEN 1 ELSE 0 END owner_eligible',
    'SCOPE',
    'PURCHASE',
    '(?::text IS NULL OR c.id=?) AND (?::text IS NULL OR c.order_id=?)',
    'ORDER BY c.due_date,c.id',
    ' JOIN (PARTIES) p ON p.id=o.party_id AND p.company_id=o.company_id',
  ]],
  ['calendar', [
    'ORDERS',
    'c.*,o.branch_id,o.department_id,OWNER',
    'c.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)',
    "INLIST AND c.status='open' AND c.due_date>=? AND c.due_date<? AND (?='team' OR c.owner_id=? OR c.created_by=?) AND (?::text IS NULL OR c.owner_id IN(SELECT user_id FROM (HRPEOPLE) hr_people WHERE id=? AND company_id=c.company_id)) AND (?='' OR c.next_action ILIKE '%'||?||'%')",
    'PURCHASE',
    'ORDER BY c.due_date,c.id LIMIT ?',
    '',
  ]],
];

describe('Sales read-tail r33 folded followup order-join generator', () => {
  test.each(cases)('%s call-site SQL stays byte-identical to the HEAD oracle', (key, args) => {
    const rendered = followupCustomerOrderTailRead(...args);
    expect(rendered).toBe(oracle[key].sql);
    expect(sha(rendered)).toBe(oracle[key].sha);
  });

  test.each(cases)('%s delegates to the primary generator with mapped arguments', (_key, args) => {
    const [orders, columns, scope, purchaseWhere, whereTail, order, joinTail] = args;
    expect(followupCustomerOrderTailRead(...args)).toBe(
      followupCustomerOrderRead(orders, columns, scope, purchaseWhere, whereTail, order, joinTail),
    );
  });

  test('one SELECT literal, one stable tail export and no new authority or lock SQL', () => {
    expect((orderRead.match(/SELECT/g) ?? []).length).toBe(1);
    expect((tailRead.match(/SELECT/g) ?? []).length).toBe(0);
    expect((tailRead.match(/export const followupCustomerOrderTailRead\s*=/g) ?? []).length).toBe(1);
    expect(followupCustomerOrderTailRead.length).toBe(6);
    expect(tailRead).toContain("import {followupCustomerOrderRead} from './followup-customer-order-read';");
    expect(tailRead).toContain('followupCustomerOrderRead(orders, columns, scope, purchaseWhere, whereTail, order, joinTail)');
    expect(followups).toContain('followupCustomerOrderTailRead(');
    expect(calendar).toContain('followupCustomerOrderTailRead(');
    for (const source of [orderRead, tailRead, followups, calendar]) {
      expect(source).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });

  test('both call sites keep their argument order and bind order', () => {
    expect(followups).toContain('scope,purchase.where,`(?::text IS NULL OR c.id=?) AND (?::text IS NULL OR c.order_id=?)`,\'ORDER BY c.due_date,c.id\',` JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id`');
    expect(followups).toContain('.bind(...args(a),...purchase.args,id??null,id??null,orderId??null,orderId??null)');
    expect(calendar).toContain("purchase.where,'ORDER BY c.due_date,c.id LIMIT ?')).bind(facts.value1,");
    expect(calendar).toContain('.bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10,...purchase.args, facts.value11)');
  });
});

describe('Sales read-tail r33 retirement check', () => {
  test('retirement:ts-statements drops from 47 to 34', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r35: delivery-presence head shared by order/return/notice guards (-1).
    expect(total).toBe(34);
    expect(total).toBeLessThan(47);
  });
});
