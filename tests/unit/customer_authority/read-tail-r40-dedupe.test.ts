import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { selectProjection } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r40-sources';
import { ownerColumnProjection } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r25-sources';
import { releaseGateRead } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r28-sources';
import { supportConclusionIdentity } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r31-sources';
import { bindingCommerceOrderSource } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r34-sources';
import { scalarProjection } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r35-sources';
import { pointRead } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r36-sources';
import { finalRootOwnerProjectionSources as commerce } from '../../../packages/server-commerce/src/public/final-root-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Closed samples copied verbatim from the pre-r40 (BEFORE=34) helper outputs.
const BINDINGS_COLS = `"id","company_id","order_id","customer_id","email","expires_at","status","reason","created_by","approved_by","reviewed_at","version","created_at","updated_at"`;
const IDENTITIES_COLS = `"binding_id","user_id","email","bound_at"`;
const PRED = "scheduled_feature_enabled('portal_release_gate.company_id','customer_authority')";
const SRC = '(SELECT ?::text company_id) portal_release_gate';
const STAFF = `SELECT o.company_id,o.branch_id,o.department_id FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id WHERE b.id=? AND b.company_id=?`;
const RESPONSE = 'SELECT id,response_version,message,created_at,actor_id FROM customer_portal_support_responses WHERE request_id=? ORDER BY response_version DESC LIMIT 1';
const REQUEST = "SELECT id FROM customer_portal_actions WHERE binding_id=? AND user_id=? AND action='request_support'";

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r40 universal projection head family', () => {
  test('renders byte-identically to every pre-r40 helper literal', () => {
    const pairs: Array<[string, string]> = [
      [ownerColumnProjection(BINDINGS_COLS, 'customer_access_bindings'), `SELECT ${BINDINGS_COLS} FROM customer_access_bindings`],
      [ownerColumnProjection(IDENTITIES_COLS, 'customer_access_identities'), `SELECT ${IDENTITIES_COLS} FROM customer_access_identities`],
      [releaseGateRead(PRED, SRC, ' allowed'), `SELECT ${PRED} allowed FROM ${SRC}`],
      [releaseGateRead(PRED, SRC, ''), `SELECT ${PRED} FROM ${SRC}`],
      [supportConclusionIdentity('ack', "'acknowledged' outcome"), "SELECT ack.request_id,ack.company_id,ack.id,'acknowledged' outcome"],
      [supportConclusionIdentity('closure', 'closure.outcome'), 'SELECT closure.request_id,closure.company_id,closure.id,closure.outcome'],
      [bindingCommerceOrderSource('o.company_id,o.branch_id,o.department_id', 'customer_access_bindings b', ' WHERE b.id=? AND b.company_id=?'), STAFF],
      [scalarProjection('(sourceQueryValue)', 'witness'), 'SELECT (sourceQueryValue) witness'],
      [pointRead('id,response_version,message,created_at,actor_id', 'customer_portal_support_responses', 'request_id=?', ' ORDER BY response_version DESC LIMIT 1'), RESPONSE],
      [pointRead('id', 'customer_portal_actions', "binding_id=? AND user_id=? AND action='request_support'"), REQUEST],
    ];
    for (const [rendered, oracle] of pairs) {
      expect(rendered).toBe(oracle);
      readOnly(rendered);
    }
  });

  test('one universal head and six delegating named wrappers', () => {
    const head = src('customer-authority-read-tail-r40-sources.ts');
    expect(head.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(head.match(/export const selectProjection/g) ?? []).toHaveLength(1);
    expect(selectProjection('id')).toBe('SELECT id');

    for (const [name, symbol] of [
      ['customer-authority-read-tail-r25-sources.ts', 'ownerColumnProjection'],
      ['customer-authority-read-tail-r28-sources.ts', 'releaseGateRead'],
      ['customer-authority-read-tail-r31-sources.ts', 'supportConclusionIdentity'],
      ['customer-authority-read-tail-r34-sources.ts', 'bindingCommerceOrderSource'],
      ['customer-authority-read-tail-r35-sources.ts', 'scalarProjection'],
      ['customer-authority-read-tail-r36-sources.ts', 'pointRead'],
    ] as Array<[string, string]>) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(0);
      expect(helper, name).toContain(`export const ${symbol}`);
      expect(helper, name).toContain('selectProjection(');
      expect(helper, name).toContain("from './customer-authority-read-tail-r40-sources'");
      expect(helper, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // r34 union fact rows keep their r38 delegation to the r25 canonical head.
    expect(src('customer-authority-read-tail-r34-sources.ts')).toContain('ownerColumnProjection(projection, from)');
  });

  test('portal probe and r39 named probe wrappers stay intact; locks, kinds and eligibility untouched', () => {
    const shared = src('customer-authority-read-sql.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(2);
    expect(shared).toContain('export const selectOneProbe');
    expect(shared).toContain('export const bindingsExists');

    const purchases = src('portal-read-purchases.ts');
    expect(purchases).toContain('selectOneProbe(');
    expect(purchases).not.toContain('SELECT 1 FROM portal_purchase_clock');
    expect(purchases).toContain('MATERIALIZED(SELECT ${packageClockSql} now_ms)');

    expect(src('portal-access.ts')).toContain('FOR UPDATE');
    expect(src('portal-source-locks.ts')).toContain('LOCK TABLE');
    expect(src('portal-purchase-binding-source.ts')).toContain('o.kind IN (${kinds})');
    expect(src('package-obligations.ts')).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");
    for (const name of [
      'assignment-eligibility-source.ts', 'manager-eligibility.ts',
      'customer-authority-read-tail-r29-sources.ts', 'portal-purchase-binding-source.ts',
      'portal-access.ts', 'portal-source-locks.ts', 'customer-authority-read-sql.ts',
    ]) {
      expect(src(name), name).not.toContain('selectProjection(');
    }
  });

  test('retirement:ts-statements is strictly below the recorded BEFORE=34', () => {
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
    expect(total).toBeLessThan(34);
    expect(total).toBe(27);
  });
});
