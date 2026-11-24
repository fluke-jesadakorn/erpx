import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { accessBindingOrderProjection } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r26-sources';
import { finalRootOwnerProjectionSources as commerceProjectionSources } from '../../../packages/server-commerce/src/adapters/final-root-owner-projection-sources';
import { finalRootOwnerProjectionSources as identityAccessProjectionSources } from '../../../packages/server-identity_access/src/adapters/final-root-owner-projection-sources';
import { salesRecipientEligibility } from '../../../packages/server-sales/src/adapters/recipient';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Closed locals reconstructed verbatim from the pre-dedupe access.ts HEAD
// (r25 baseline, BEFORE=60). Only the owner projection/binding-to-order join
// and trailing clause are parameterized; emitted text is pinned byte-for-byte.
const scope = 'o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)';
const eligible = salesRecipientEligibility().replaceAll('c.owner_id', 'm.user_id');
const reviewers = `m.company_id=o.company_id AND m.user_id<>b.created_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'commerce.approve') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=o.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=o.department_id)) AND ${eligible}`;
const actorEligible = `CASE WHEN EXISTS(SELECT 1 FROM (${identityAccessProjectionSources.assignments}) m WHERE ${reviewers} AND m.user_id=?) THEN 1 ELSE 0 END actor_eligible`;
const assignmentSource = (projection = '1') => `SELECT ${projection} FROM (${identityAccessProjectionSources.assignments})`;
const orders = commerceProjectionSources.commerce_orders;

const DETAIL = `SELECT b.*,o.number,o.state order_state,o.branch_id,o.department_id,${actorEligible} FROM customer_access_bindings b JOIN (${orders}) o ON o.id=b.order_id AND o.company_id=b.company_id WHERE b.id=? AND ${scope}`;
const ROWS = `SELECT b.*,o.number,o.state order_state,${actorEligible},(${assignmentSource('COUNT(DISTINCT m.user_id)')} m WHERE ${reviewers}) reviewers FROM customer_access_bindings b JOIN (${orders}) o ON o.id=b.order_id AND o.company_id=b.company_id WHERE ${scope} ORDER BY b.updated_at DESC,b.id`;

const detailArgs = [orders, `o.branch_id,o.department_id,${actorEligible}`, `WHERE b.id=? AND ${scope}`] as const;
const rowsArgs = [orders, `${actorEligible},(${assignmentSource('COUNT(DISTINCT m.user_id)')} m WHERE ${reviewers}) reviewers`, `WHERE ${scope} ORDER BY b.updated_at DESC,b.id`] as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r26 binding/order row projection family', () => {
  test('renders byte-identically to both pre-dedupe literals', () => {
    expect(accessBindingOrderProjection(...detailArgs)).toBe(DETAIL);
    expect(accessBindingOrderProjection(...rowsArgs)).toBe(ROWS);
    readOnly(DETAIL);
    readOnly(ROWS);
  });

  test('one shared literal, no inline copies, and pinned caller arguments', () => {
    const shared = src('customer-authority-read-tail-r26-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(shared).toContain('export const accessBindingOrderProjection');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const caller = src('access.ts');
    expect(caller).not.toContain(`SELECT b.*,o.number,o.state order_state,o.branch_id`);
    expect(caller.match(/accessBindingOrderProjection\(/g) ?? []).toHaveLength(2);
    expect(caller).toContain('accessBindingOrderProjection(commerceProjectionSources.commerce_orders,');
  });

  test('r21-r25 builders, source locks and native kinds stay untouched', () => {
    const r21 = src('customer-authority-read-sql.ts');
    expect(r21.match(/\bSELECT\b/g) ?? []).toHaveLength(2);
    expect(r21).toContain('export const jsonbAggFact');
    expect(r21).toContain('export const bindingsExists');

    const r22 = src('package-obligation-sql.ts');
    expect(r22.match(/\bSELECT\b/g) ?? []).toHaveLength(3);
    expect(r22).toContain('export const latestSupportVersion');
    expect(r22).toContain('export const supportAbsent');
    expect(r22).toContain('export const commandAuditProbe');

    const r23 = src('customer-authority-read-tail-r23-sources.ts');
    expect(r23.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r23).toContain('export const jsonbImageProjection');

    const r24 = src('customer-authority-read-tail-r24-sources.ts');
    expect(r24.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r24).toContain('export const bindingFactProjection');

    const r25 = src('customer-authority-read-tail-r25-sources.ts');
    expect(r25.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r25).toContain('export const ownerColumnProjection');

    // Both original repository reads preserve their trailing clauses and binds.
    const caller = src('access.ts');
    for (const ref of ['WHERE b.id=? AND ${scope}', 'ORDER BY b.updated_at DESC,b.id', '.bind(a.userId', 'function repository']) {
      expect(caller, ref).toContain(ref);
    }
    expect(caller).not.toMatch(/FOR\s+UPDATE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=60', () => {
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
    expect(total).toBeLessThan(60);
    expect(total).toBe(59);
  });
});
