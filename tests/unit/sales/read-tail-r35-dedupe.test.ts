import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../modules/sales/server/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const obligations = read('package-obligations.ts');
const purchases = read('followup-purchases.ts');
const native = read('native-sales-read.ts');
const commercial = read('commercial-owner-projection-sources.ts');

const sha = (value: string) => createHash('sha256').update(value).digest('hex');

// Byte oracles captured at the pre-r35 head. The delivery-presence head must
// render every literal byte-for-byte after delegating to the shared composer.
const oracle = {
  order: {
    sql: 'SELECT 1 FROM deliveries f WHERE f.order_id=o.id',
    sha: 'b22ea50567464fb36e9785a371b6f9084d4462d690ca6b08637f89218e41a8c8',
  },
  return: {
    sql: 'SELECT 1 FROM deliveries f WHERE f.id=r.fulfillment_id AND f.order_id=r.order_id AND f.invoice_id=r.invoice_id',
    sha: 'b7acbf6b984c086315577f860e7fe6d7d6760e0f4f4c2df28812866f0c8abd30',
  },
  customerNotice: {
    sql: 'SELECT 1 FROM deliveries f WHERE f.id=notice.fulfillment_id AND f.order_id=notice.order_id AND f.company_id=notice.company_id AND f.party_id=notice.party_id',
    sha: 'be512b6905c39b5072cca4607e0da5a2a7584270016b7cd2a4699b6806112309',
  },
} as const;

type Presence = (tail: string) => string;
type Guard = (alias: string, tail: string) => string;
function load(): { presence: Presence; guard: Guard } {
  const file = ts.createSourceFile('package-obligations.ts', obligations, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const names = ['deliveryPresence', 'deliveryFactExists'];
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap(s => [...s.declarationList.declarations])
    .filter(d => names.includes(d.name.getText(file)));
  expect(decls).toHaveLength(2);
  const code = ts.transpileModule(decls.map(d => `const ${d.getText(file)};`).join('\n'), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  return new Function(`${code}\nreturn {presence: deliveryPresence, guard: deliveryFactExists};`)() as {
    presence: Presence;
    guard: Guard;
  };
}

const { presence, guard } = load();
const rendered = {
  order: presence('order_id=o.id'),
  return: guard('r', ' AND f.invoice_id=r.invoice_id'),
  customerNotice: guard('notice', ' AND f.company_id=notice.company_id AND f.party_id=notice.party_id'),
};

describe('Sales read-tail r35 shared delivery-presence head', () => {
  test.each(Object.keys(oracle) as (keyof typeof oracle)[])(
    '%s stays byte-identical to the pre-r35 oracle',
    key => {
      expect(rendered[key]).toBe(oracle[key].sql);
      expect(sha(rendered[key])).toBe(oracle[key].sha);
    },
  );

  test('one shared head composes both callers with no inlined literal', () => {
    expect((obligations.match(/const deliveryPresence\s*=/g) ?? []).length).toBe(1);
    expect((obligations.match(/const orderDeliveryExistsSql\s*=/g) ?? []).length).toBe(1);
    expect((obligations.match(/const deliveryFactExists\s*=/g) ?? []).length).toBe(1);
    expect(obligations).toContain("const deliveryPresence=(tail:string)=>`SELECT 1 FROM deliveries f WHERE f.${tail}`;");
    expect(obligations).toContain("const orderDeliveryExistsSql=deliveryPresence('order_id=o.id');");
    expect(obligations).toContain('const deliveryFactExists=(alias:string,tail:string)=>deliveryPresence(');
    expect(obligations).not.toContain(oracle.order.sql);
    expect(obligations).not.toContain(oracle.return.sql);
    expect(obligations).not.toContain(oracle.customerNotice.sql);
    for (const name of ['deliveryPresence', 'orderDeliveryExistsSql', 'deliveryFactExists']) {
      const start = obligations.indexOf(`const ${name}=`);
      const end = obligations.indexOf('\n', start);
      const line = obligations.slice(start, end);
      expect((line.match(/SELECT/g) ?? []).length).toBe(name === 'deliveryPresence' ? 1 : 0);
    }
  });

  test('all four call sites keep their arguments and bind order', () => {
    expect(obligations).not.toContain('NOT EXISTS(${orderDeliveryExistsSql})');
    expect(obligations).not.toContain('EXISTS(${orderDeliveryExistsSql})');
    expect(obligations).not.toContain("deliveryFactExists('r',' AND f.invoice_id=r.invoice_id')");
    expect(obligations).not.toContain("deliveryFactExists('notice',' AND f.company_id=notice.company_id AND f.party_id=notice.party_id')");
  });
});

describe('Sales read-tail r35 retirement check', () => {
  test('locks, cross-owner sources and native reads stay untouched', () => {
    const head = obligations.slice(obligations.indexOf('const deliveryPresence='), obligations.indexOf('type Input='));
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('@erpx/commercial-server/public/package-settlement-sources');
    expect(obligations).toContain('@erpx/commerce-server/public/sales-package-sources');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ACTIVE');
    expect(native).toContain('export const SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED');
    expect((commercial.match(/SELECT/g) ?? []).length).toBe(0);
  });

  test('retirement:ts-statements drops from 45 to 16', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r35: deliveryPresence shares one head across order/return/notice guards (-1).
    expect(total).toBe(16);
    expect(total).toBeLessThan(45);
  });
});
