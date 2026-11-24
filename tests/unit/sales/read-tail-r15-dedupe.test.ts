import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { commercialOwnerProjectionSources } from '../../../packages/server-sales/src/adapters/commercial-owner-projection-sources';
import { finalRootOwnerProjectionSources } from '../../../packages/server-sales/src/adapters/final-root-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const src = readFileSync(resolve(srcRoot, 'adapters/final-root-owner-projection-sources.ts'), 'utf8');
const commercialSrc = readFileSync(resolve(srcRoot, 'adapters/commercial-owner-projection-sources.ts'), 'utf8');

// Pre-dedupe literals copied from HEAD, used only as the byte-identity oracle.
const pre = {
  customer_contact_note_history: 'SELECT "id","note_id","version","action","body","reason","actor_id","created_at" FROM customer_contact_note_history',
  customer_contact_notes: 'SELECT "id","company_id","customer_id","order_id","channel","occurred_at","body","status","version","created_by","created_at","updated_at" FROM customer_contact_notes',
  customer_followup_events: 'SELECT "id","followup_id","version","action","actor_id","owner_id","due_date","reason","created_at" FROM customer_followup_events',
} as const;

describe('Sales read-tail r15 tableRead family', () => {
  test.each(Object.entries(pre) as [keyof typeof pre, string][])(
    '%s stays byte-identical to the pre-dedupe literal',
    (key, literal) => {
      const actual = finalRootOwnerProjectionSources[key];
      expect(actual).toBe(literal);
      expect(createHash('sha256').update(actual).digest('hex')).toBe(
        createHash('sha256').update(literal).digest('hex'),
      );
    },
  );

  test('the family is one generator literal with three callers and no inlining', () => {
    expect(src.match(/const tableRead\s*=/g) ?? []).toHaveLength(1);
    expect(src.match(/tableRead\('/g) ?? []).toHaveLength(3);
    expect(src).not.toMatch(/^  (customer_contact_note_history|customer_contact_notes|customer_followup_events): "SELECT /m);
    const family = src.slice(src.indexOf('const tableRead='), src.indexOf('export const finalRootOwnerProjectionSources'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r3 cross-file aliases and their locked commercial literals stay untouched', () => {
    for (const key of ['commerce_customer_notifications', 'customer_followups'] as const) {
      expect(commercialSrc).toMatch(new RegExp(`^  ${key}: "SELECT `, 'm'));
      expect(src).toContain(`${key}: commercialOwnerProjectionSources.${key},`);
      expect(finalRootOwnerProjectionSources[key]).toBe(commercialOwnerProjectionSources[key]);
    }
    expect(Object.keys(finalRootOwnerProjectionSources)).toEqual([
      'commerce_customer_notifications',
      'customer_contact_note_history',
      'customer_contact_notes',
      'customer_followup_events',
      'customer_followups',
    ]);
  });

  test('source locks stay independent and untouched', () => {
    const followup = readFileSync(resolve(srcRoot, 'adapters/followup-purchases.ts'), 'utf8');
    const pricing = readFileSync(resolve(srcRoot, 'adapters/pricing-purchases.ts'), 'utf8');
    const obligations = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');
    expect(followup).toContain("LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE");
    expect(pricing).toContain("LOCK TABLE sales_price_lists,sales_price_list_items IN SHARE ROW EXCLUSIVE MODE");
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
    expect(src).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE/i);
  });
});

describe('Sales read-tail r15 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r15 BEFORE=73 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(73);
  });
});
