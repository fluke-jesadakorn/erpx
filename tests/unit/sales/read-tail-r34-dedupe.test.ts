import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { byKey } from '../../../packages/server-sales/src/adapters/by-key';
import { tableRead } from '../../../packages/server-sales/src/adapters/followup-source-sql';
import { pointRead } from '../../../packages/server-sales/src/adapters/point-read';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const pointReadSource = read('point-read.ts');
const followupSource = read('followup-source-sql.ts');
const byKeySource = read('by-key.ts');
const workflowSource = read('workflow-notification-source.ts');
const followups = read('followups.ts');
const purchases = read('followup-purchases.ts');
const obligations = read('package-obligations.ts');
const native = read('native-sales-read.ts');

const sha = (value: string) => createHash('sha256').update(value).digest('hex');

// Byte oracles captured at the pre-r34 head. pointRead keeps rendering every
// literal byte-for-byte after delegating to the owner-local tableRead composer.
const oracle = {
  notification: {
    sql: 'SELECT id FROM commerce_customer_notifications WHERE fulfillment_id=?',
    sha: '88c9880faee3b78165f52cde602412cdfd77d3474014d8595efdba5effcd868b',
  },
  followupHistory: {
    sql: 'SELECT * FROM customer_followup_events WHERE followup_id=? ORDER BY version DESC',
    sha: '940dfc690909479731137f4425caab98b4d6a4d77407a902627df9c361624e16',
  },
  point: {
    sql: 'SELECT * FROM T WHERE C=?',
    sha: '882bb7570e2e65994f1256d7b89d6c141f036ec857b0ddef642ae78246a8db8a',
  },
  pointOrdered: {
    sql: 'SELECT C1,C2 FROM T WHERE C=? ORDER BY X',
    sha: '0fc32212b7318c03c8198163ba36e480af1dd58aa853e223132950cd9c3d80ea',
  },
  byKeySettings: {
    sql: 'SELECT * FROM commerce_customer_notification_settings WHERE company_id=?',
    sha: '8f3843b3d170743fc12de888c4e4845da326bcd815db695c6986f02481a53242',
  },
  byKeyItems: {
    sql: 'SELECT * FROM sales_price_list_items WHERE price_list_id=?',
    sha: '9b0969157e73761ab0092356994690afa36acf6e787ed9aa000c8217c8ccdf4a',
  },
} as const;

const rendered = {
  notification: pointRead('id', 'commerce_customer_notifications', 'fulfillment_id'),
  followupHistory: pointRead('*', 'customer_followup_events', 'followup_id', ' ORDER BY version DESC'),
  point: pointRead('*', 'T', 'C'),
  pointOrdered: pointRead('C1,C2', 'T', 'C', ' ORDER BY X'),
  byKeySettings: byKey('commerce_customer_notification_settings', 'company_id'),
  byKeyItems: byKey('sales_price_list_items', 'price_list_id'),
};

type RenderKey = keyof typeof oracle;

describe('Sales read-tail r34 pointRead folded onto tableRead', () => {
  test.each(Object.keys(oracle) as RenderKey[])('%s call-site SQL stays byte-identical to the pre-r34 oracle', key => {
    expect(rendered[key]).toBe(oracle[key].sql);
    expect(sha(rendered[key])).toBe(oracle[key].sha);
  });

  test('pointRead delegates to tableRead and reproduces the head literal exactly', () => {
    expect((pointReadSource.match(/export const pointRead\s*=/g) ?? []).length).toBe(1);
    expect((pointReadSource.match(/SELECT/g) ?? []).length).toBe(0);
    expect(pointReadSource).toContain("import {tableRead} from './followup-source-sql';");
    expect(pointReadSource).toContain('`${tableRead(columns,table)} WHERE ${column}=?${order}`');
    expect((followupSource.match(/export const tableRead\s*=/g) ?? []).length).toBe(1);
    expect(followupSource).toContain('export const tableRead=(columns:string,table:string)=>`SELECT ${columns} FROM ${table}`;');
    expect(tableRead('C', 'T')).toBe('SELECT C FROM T');
    for (const args of [
      ['*', 'T', 'C', ''],
      ['C1,C2', 'T', 'C', ' ORDER BY X'],
      ['id', 'commerce_customer_notifications', 'fulfillment_id', ''],
      ['*', 'customer_followup_events', 'followup_id', ' ORDER BY version DESC'],
    ] as [string, string, string, string][]) {
      expect(pointRead(args[0], args[1], args[2], args[3])).toBe(
        `${tableRead(args[0], args[1])} WHERE ${args[2]}=?${args[3]}`,
      );
    }
  });

  test('all three callers and their bind order stay unchanged', () => {
    expect(byKeySource).toContain('export const byKey =');
    expect(byKeySource).toContain("pointRead('*', table, column)");
    expect((byKeySource.match(/SELECT/g) ?? []).length).toBe(0);
    expect(workflowSource).toContain("pointRead('id','commerce_customer_notifications','fulfillment_id')");
    expect(workflowSource).toContain('.bind(fulfillmentId)');
    expect(followups).toContain("pointRead('*','customer_followup_events','followup_id',' ORDER BY version DESC')");
    expect(followups).toContain('.bind(id).all<Row>()');
  });
});

describe('Sales read-tail r34 retirement check', () => {
  test('locks, cross-owner sources and native reads stay untouched', () => {
    expect(pointReadSource).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
    expect(followupSource).toContain('@erpx/server-people_payroll/public/commercial-owner-projection-sources');
    expect(followupSource).toContain('@erpx/server-commercial/public/purchased-capability-sql');
    expect(followupSource).toContain('export const authorityScopeRead=');
    expect(followupSource).toContain('export const clockNowMs=');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ACTIVE');
    expect(native).toContain('export const SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED');
  });

  test('retirement:ts-statements drops from 46 to 36', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r34: pointRead no longer inlines SELECT; it delegates to tableRead (-1).
    // r35: delivery-presence head shared by order/return/notice guards (-1).
    expect(total).toBe(36);
    expect(total).toBeLessThan(46);
  });
});
