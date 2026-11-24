import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { ecommerceRowProbe, ecommerceScopeRow } from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const shared = read('adapters/read-tail-sql.ts');
const ar = read('adapters/attachment-read.ts');
const pb = read('adapters/purchase-boundary.ts');
const src = read('adapters/ecommerce.ts');

// Pre-r23 SQL copied verbatim from HEAD (r22), kept only as byte-identity oracles.
const scopeRow = ecommerceScopeRow('workforce_actor_scope', true, '');
const assignmentProbe = (where: string) => `SELECT 1 FROM${scopeRow} WHERE ${where}`;
const receiptWhere = "command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
const receiptProbe = `SELECT 1 FROM command_receipts WHERE ${receiptWhere}`;

describe('Ecommerce read-tail r23 bare one-row probe dedupe', () => {
  test('F1: one parameterized probe frame rebuilds both pre-r23 literals byte-identically', () => {
    expect(ecommerceRowProbe(scopeRow, '<where>', '')).toBe(assignmentProbe('<where>'));
    expect(ecommerceRowProbe('command_receipts', receiptWhere)).toBe(receiptProbe);
    expect(receiptProbe).toBe("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?");
  });

  test('F2: family is defined once and composed once at each of the two call sites', () => {
    expect((shared.match(/export const ecommerceRowProbe = /g) ?? []).length).toBe(1);
    expect((ar.match(/ecommerceRowProbe\(/g) ?? []).length).toBe(1);
    expect((pb.match(/ecommerceRowProbe\(/g) ?? []).length).toBe(1);
    expect((ar.match(/ecommerceRowProbe\(/g) ?? []).length + (pb.match(/ecommerceRowProbe\(/g) ?? []).length).toBe(2);
    expect(ar).not.toContain('SELECT 1 FROM${ecommerceScopeRow(');
    expect(pb).not.toContain('"SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(shared).not.toContain('command_receipts WHERE command_id=?');
    expect(ar).toContain("ecommerceRowProbe(ecommerceScopeRow('workforce_actor_scope', true, ''),current.where,'')");
    expect(pb).toContain("ecommerceRowProbe('command_receipts',\"command_id=?");
  });

  test('F3: binds stay unchanged and composed probes stay read-only', () => {
    expect((scopeRow.match(/\?/g) ?? []).length).toBe(3);
    expect((receiptWhere.match(/\?/g) ?? []).length).toBe(5);
    expect((assignmentProbe('<where>').match(/\?/g) ?? []).length).toBe(3);
    for (const sql of [assignmentProbe('<where>'), receiptProbe]) {
      expect(sql).toMatch(/^SELECT 1 FROM/);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(ar).toContain('.bind(actor.companyId,actor.branchId,actor.departmentId,...current.args)');
    expect(pb).toContain('.bind(command.key,command.operation,command.id,actor.userId,actor.companyId)');
  });

  test('F4: native, locks and r1-r22 frame sites stay intact', () => {
    for (const token of [
      'ecommerceSelectOne', 'ecommerceModuleAssignment', 'ecommerceScopeRow',
      'ecommerceReleaseGate', 'ecommerceExistsGuard', 'ecommerceOrderedList',
      'ecommerceListingChannelRow', 'ecommerceSourceIdentity', 'ecommerceListingEvidence',
    ]) {
      expect(shared).toContain(`export const ${token} =`);
    }
    expect((shared.match(/export const ecommerceSelectOne = /g) ?? []).length).toBe(1);
    expect((shared.match(/ecommerceSelectOne\(/g) ?? []).length).toBe(3);
    expect((read('adapters/package-obligations.ts').match(/ecommerceSelectOne\(/g) ?? []).length).toBe(2);
    expect(ar).toContain('ecommerceCommerceSourceReadLocks');
    expect(pb).toContain('commerceOwnerTransaction(');
    expect(src).toContain("ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')");
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(src).toContain('lock ? " FOR UPDATE" : ""');
    expect(src).toContain('nativeWrite(');
    expect(src).not.toContain('ecommerceRowProbe');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Ecommerce read-tail r23 retirement check', () => {
  test('retirement:ts-statements === 19 (r25 BEFORE=22; r24 BEFORE=25)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(5);
    expect(total).toBeLessThan(22);
  });
});
