import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

import { commercialOwnerProjectionSources } from '../../../packages/server-sales/src/adapters/commercial-owner-projection-sources';
import { finalRootOwnerProjectionSources } from '../../../packages/server-sales/src/adapters/final-root-owner-projection-sources';
import { tableRead } from '../../../packages/server-sales/src/adapters/followup-source-sql';
import { priceListSelect } from '../../../packages/server-sales/src/adapters/pricing-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const commercial = read('commercial-owner-projection-sources.ts');
const finalRoot = read('final-root-owner-projection-sources.ts');
const shared = read('followup-source-sql.ts');
const pricing = read('pricing.ts');
const pricingSql = read('pricing-source-sql.ts');

const sha = (value: string) => createHash('sha256').update(value).digest('hex');

// Pre-r30 inline literals copied from HEAD. Only the composition moved; every
// column, bind and relation stays byte-identical under the shared generators.
const pre = {
  commerce_customer_notification_settings: 'SELECT "company_id","adapter","version","updated_by","updated_at" FROM commerce_customer_notification_settings',
  commerce_customer_notifications: 'SELECT "id","company_id","branch_id","order_id","fulfillment_id","party_id","party_version","recipient_email","subject","body_text","adapter","status","version","attempts","next_attempt_at","lease_until","provider_message_id","sent_at","last_error","created_by","created_at","updated_at" FROM commerce_customer_notifications',
  customer_followups: 'SELECT "id","company_id","order_id","owner_id","due_date","next_action","status","outcome","version","created_by","created_at","updated_at" FROM customer_followups',
} as const;
const preSha = {
  commerce_customer_notification_settings: 'ba12dfdadd16a78f52b1d00de2444f9023e6feb52d2083f9cc2f12471dd9798a',
  commerce_customer_notifications: '5d98b0b949677262c4d6fbca156574e0ed68c0026322a282e98d954939604f86',
  customer_followups: 'a6bb226508b1c10292bb4f8c612a6e97bdd0b5c393d0f9beb2d54f37dfbe26f9',
} as const;

// The untouched r15 final-root family, re-checked through the shared alias.
const finalPre = {
  customer_contact_note_history: 'SELECT "id","note_id","version","action","body","reason","actor_id","created_at" FROM customer_contact_note_history',
  customer_contact_notes: 'SELECT "id","company_id","customer_id","order_id","channel","occurred_at","body","status","version","created_by","created_at","updated_at" FROM customer_contact_notes',
  customer_followup_events: 'SELECT "id","followup_id","version","action","actor_id","owner_id","due_date","reason","created_at" FROM customer_followup_events',
} as const;
const finalSha = {
  customer_contact_note_history: '40cf94521a4da22276882f76fdbd8a7a0378d653d5864a7a0a8c07553c3b1690',
  customer_contact_notes: '3428a0e4fc53dd82fab46d095b8ac4dc3f79f8d8eda4f1a6e52b43d79c8a55a2',
  customer_followup_events: '47e069faf399b6b2894e800fb5d616cfc6f3f15fe2f357df71181c9fd3ac5aae',
} as const;

// Pre-r30 price-list head renders. `${activeCurrencySql}` is an owner-local
// token left uninterpolated so the head itself is compared byte for byte.
const pricePre = {
  byId: 'SELECT * FROM sales_price_lists WHERE id=? AND company_id=?',
  active: "SELECT * FROM sales_price_lists WHERE company_id=? AND status='active' AND currency=${activeCurrencySql} AND valid_from<=? AND valid_until>=? AND (customer_id=? OR customer_id IS NULL) ORDER BY CASE WHEN customer_id=? THEN 0 ELSE 1 END,valid_from DESC,activated_at DESC LIMIT 1",
} as const;
const priceSha = {
  byId: 'e392053f5374623de9070539bc8d485bd767b6358821040b043b54b1f601d5e5',
  active: 'c1f08fb6c17f9a166003b1e82abd8ab7d165fba373270d40c39cdc21b62417e1',
} as const;

const split = (literal: string): [string, string] => {
  const at = literal.indexOf(' FROM ');
  return [literal.slice('SELECT '.length, at), literal.slice(at + ' FROM '.length)];
};

const importsOf = (text: string): string[] => {
  const ast = ts.createSourceFile('source.ts', text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  return ast.statements
    .filter(ts.isImportDeclaration)
    .map(s => (s.moduleSpecifier as ts.StringLiteral).text);
};

describe('Sales read-tail r30 tableRead family', () => {
  test.each(Object.keys(pre) as (keyof typeof pre)[])(
    '%s projection stays byte-identical to the pre-r30 inline literal',
    key => {
      const actual = commercialOwnerProjectionSources[key];
      expect(actual).toBe(pre[key]);
      expect(sha(actual)).toBe(preSha[key]);
    },
  );

  test.each(Object.keys(finalPre) as (keyof typeof finalPre)[])(
    '%s final-root projection stays byte-identical through the shared alias',
    key => {
      const actual = finalRootOwnerProjectionSources[key];
      expect(actual).toBe(finalPre[key]);
      expect(sha(actual)).toBe(finalSha[key]);
    },
  );

  test('one shared generator renders all three pre-r30 literals byte-for-byte', () => {
    expect(typeof tableRead).toBe('function');
    expect(tableRead.length).toBe(2);
    for (const key of Object.keys(pre) as (keyof typeof pre)[]) {
      const [columns, table] = split(pre[key]);
      expect(tableRead(columns, table)).toBe(pre[key]);
      expect(sha(tableRead(columns, table))).toBe(preSha[key]);
    }
    expect((shared.match(/const tableRead\s*=/g) ?? []).length).toBe(1);
    expect((shared.match(/SELECT/g) ?? []).length).toBe(6);
    const family = shared.slice(shared.indexOf('const tableRead='), shared.indexOf('export const ownerNameProjection='));
    expect((family.match(/SELECT/g) ?? []).length).toBe(1);
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('commercial and final-root compose through the shared module without a cycle', () => {
    expect((commercial.match(/SELECT/g) ?? []).length).toBe(0);
    expect((commercial.match(/tableRead\(/g) ?? []).length).toBe(3);
    expect(commercial).not.toMatch(/^  \w+: "SELECT /m);
    expect((finalRoot.match(/SELECT/g) ?? []).length).toBe(0);
    expect((finalRoot.match(/const tableRead\s*=/g) ?? []).length).toBe(1);
    expect(finalRoot).toContain('const tableRead = sharedTableRead;');
    expect((finalRoot.match(/tableRead\(/g) ?? []).length).toBe(3);
    expect(finalRoot).toContain('commerce_customer_notifications: commercialOwnerProjectionSources.commerce_customer_notifications,');
    expect(finalRoot).toContain('customer_followups: commercialOwnerProjectionSources.customer_followups,');

    const commercialImports = importsOf(commercial);
    const finalImports = importsOf(finalRoot);
    const sharedImports = importsOf(shared);
    expect(commercialImports).toContain('./followup-source-sql');
    expect(finalImports).toContain('./followup-source-sql');
    expect(finalImports).toContain('./commercial-owner-projection-sources');
    expect(sharedImports).not.toContain('./commercial-owner-projection-sources');
    expect(sharedImports).not.toContain('./final-root-owner-projection-sources');
    expect(sharedImports).not.toContain('./pricing-source-sql');
  });
});

describe('Sales read-tail r30 priceListSelect family', () => {
  test.each(Object.keys(pricePre) as (keyof typeof pricePre)[])(
    '%s price-list head stays byte-identical to the pre-r30 render',
    key => {
      const tail = pricePre[key].slice('SELECT * FROM sales_price_lists '.length);
      const rendered = priceListSelect(tail);
      expect(rendered).toBe(pricePre[key]);
      expect(sha(rendered)).toBe(priceSha[key]);
    },
  );

  test('one closed generator with two callers and no inlined head', () => {
    expect(typeof priceListSelect).toBe('function');
    expect(priceListSelect.length).toBe(1);
    expect((pricingSql.match(/priceListSelect\s*=/g) ?? []).length).toBe(1);
    expect((pricingSql.match(/SELECT/g) ?? []).length).toBe(1);
    expect(pricingSql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect((pricing.match(/priceListSelect\(/g) ?? []).length).toBe(2);
    expect(pricing).not.toMatch(/`SELECT \* FROM sales_price_lists /);
    expect(pricing).toContain("const priceListByIdSql=priceListSelect('WHERE id=? AND company_id=?');");
    expect(pricing).toContain('db.prepare(priceListSelect(`WHERE company_id=? AND status=');
    expect(importsOf(pricing)).toContain('./pricing-source-sql');
    expect(importsOf(pricingSql)).toEqual([]);
  });
});

describe('Sales read-tail r30 retirement check', () => {
  test('binds, locks, authorityScopeRead and native reads stay untouched', () => {
    const obligations = read('package-obligations.ts');
    const purchases = read('followup-purchases.ts');
    const pricingPurchases = read('pricing-purchases.ts');
    const native = read('native-sales-read.ts');
    const rustRegistry = readFileSync(resolve(srcRoot, 'adapters/native_sales_read_sql.rs'), 'utf8');
    expect(shared).toContain('export const authorityScopeRead=');
    expect(shared).toContain('export const clockNowMs=');
    expect(shared).toContain('export const followupEvidencePrefix=');
    expect(shared).toContain('export const followupMetricPrefix=');
    expect(shared).toContain("export const actorScopeTuple=scopeTuple('actor');");
    expect(pricingPurchases).toContain('authorityScopeRead(');
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(pricingPurchases).toContain('LOCK TABLE sales_price_lists,sales_price_list_items IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
    expect(native).toContain('export const SALES_RD_PRICE_LIST_ACTIVE');
    expect((rustRegistry.match(/pub const RD_\w+: &str =/g) ?? []).length).toBe(4);
    expect(shared).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE/i);
    expect(commercial).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE/i);
    expect(finalRoot).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE/i);
  });

  test('retirement:ts-statements stays strictly below the r29 BEFORE=54 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r30 shares tableRead (-3) and priceListSelect (-1).
    expect(total).toBe(50);
    expect(total).toBeLessThan(54);
  });
});
