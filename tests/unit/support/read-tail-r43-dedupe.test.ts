import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { readTailR43ColumnsFromSql } from '../../../modules/support/server/src/adapters/read-tail-r43-sql';

const srcRoot = resolve(__dirname, '../../../modules/support/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const totalSelects = () => {
  let total = 0;
  for (const rel of readdirSync(srcRoot, { recursive: true })) {
    if (!String(rel).endsWith('.ts')) continue;
    total += readFileSync(resolve(srcRoot, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
  }
  return total;
};

/** Byte-identical HEAD oracles captured before the cutover. Expected SQL is
 * written out (or composed from the retained HEAD valid-binding source), never
 * derived from the renderer, so a head, column, tail, bind or whitespace
 * regression fails the oracle. The renderer moves only the shared
 * `SELECT ${columns} FROM ` prefix; every projected column, trailing clause,
 * bind, lock and native kind stays at its call site. */
const scope = 't.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';
const incidentWorkspace =
  `SELECT t.* FROM support_incidents t WHERE ${scope} AND t.active=1 ORDER BY t.updated_at DESC,t.id LIMIT 200`;
const incidentTickets =
  `SELECT t.id,t.title,t.version,t.status FROM support_tickets t WHERE ${scope} AND t.source_kind='sales_order' ORDER BY t.updated_at DESC,t.id LIMIT 200`;
const identitySql = (alias: string) =>
  `jsonb_build_object('companyId',${alias}.company_id,'branchId',${alias}.branch_id,'departmentId',${alias}.department_id,'sourceKind',${alias}.source_kind,'orderId',${alias}.order_id,'customerId',${alias}.customer_id,'sourceRecord',${alias}.source_record_json)`;
/** Exact retired valid-binding subquery; the head now comes from Rust. */
const validBefore = `SELECT binding.* FROM support_ticket_portal_bindings binding JOIN support_tickets ticket ON ticket.id=binding.ticket_id AND ticket.company_id=binding.company_id AND ticket.source_kind<>'field_service' AND binding.source_identity=${identitySql('ticket')} JOIN command_receipts birth ON birth.command_id=binding.prepare_command_id AND birth.operation='support.ticket-portal.prepare' AND birth.record_id=binding.id AND birth.actor_id=binding.prepared_by AND birth.authority_json::jsonb->>'companyId'=binding.company_id AND birth.result_json::jsonb->>'id'=binding.id AND birth.result_json::jsonb->>'status'='draft' AND birth.result_json::jsonb->>'version'='1'`;
const portalRead = `SELECT binding.* FROM (${validBefore}) binding`;
const portalStar = `SELECT * FROM (${validBefore}) binding`;

const oracles = [
  { name: 'incident workspace list', rendered: readTailR43ColumnsFromSql('t.*', `support_incidents t WHERE ${scope} AND t.active=1 ORDER BY t.updated_at DESC,t.id LIMIT 200`), sql: incidentWorkspace, binds: 5 },
  { name: 'incident ticket list', rendered: readTailR43ColumnsFromSql('t.id,t.title,t.version,t.status', `support_tickets t WHERE ${scope} AND t.source_kind='sales_order' ORDER BY t.updated_at DESC,t.id LIMIT 200`), sql: incidentTickets, binds: 5 },
  { name: 'ticket-portal valid binding read', rendered: readTailR43ColumnsFromSql('binding.*', `(${validBefore}) binding`), sql: portalRead, binds: 0 },
  { name: 'ticket-portal staff star read', rendered: readTailR43ColumnsFromSql('*', `(${validBefore}) binding`), sql: portalStar, binds: 0 },
] as const;

describe('support read-tail r43 byte-identical column-projection head', () => {
  test.each(oracles)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql, binds }) => {
    expect(rendered).toBe(sql);
    expect(Buffer.byteLength(rendered)).toBe(Buffer.byteLength(sql));
    expect(sha256(rendered)).toBe(sha256(sql));
    expect(rendered.match(/\?/g) ?? []).toHaveLength(binds);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the owner renderer owns exactly the one shared read token', () => {
    const helper = src('read-tail-r43-sql.ts');
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper.match(/readTailR43ColumnsFromSql =/g)).toHaveLength(1);
    expect(readTailR43ColumnsFromSql('x', 'y')).toBe('SELECT x FROM y');
  });

  test('both consumer sites route their former inline head through the renderer', () => {
    const incidents = src('support-incidents.ts');
    expect(incidents).toContain("from './read-tail-r43-sql'");
    expect(incidents).toContain('readTailR43ColumnsFromSql(columns, `${table} t WHERE ${scope} AND ${extra} ORDER BY t.updated_at DESC,t.id LIMIT 200`)');
    expect(incidents).not.toContain('`SELECT ${columns} FROM ${table} t WHERE');

    const portal = src('ticket-portal-sources.ts');
    expect(portal).toContain("from './read-tail-r43-sql'");
    expect(portal).toContain('readTailR43ColumnsFromSql(columns,`(${head}) binding`)');
    expect(portal).toContain('nativeSupportReadFragment(SUPPORT_RD_TICKET_PORTAL_VALID_BINDINGS,tailBinds)');
    expect(portal).not.toContain('supportTicketPortalValidBindings');
  });

  test('native registry kinds, locks, advisory, eligibility wrappers and r18-r42 helpers stay untouched', () => {
    const registry = src('native_support_read_sql.rs');
    const bridge = src('native-support-read.ts');
    expect(registry).toContain('assert_eq!(STATEMENTS.len(), 55);');
    expect(registry).not.toMatch(/LOCK TABLE|pg_advisory_xact_lock/);
    expect(bridge).toContain('SUPPORT_RD_TICKET_WORK');
    expect(bridge).toContain('SUPPORT_RD_INCIDENT_WORK');
    const ticket = src('support-ticket-read.ts');
    expect(ticket).toContain('SELECT id FROM support_tickets WHERE id=? AND company_id=? FOR SHARE');
    const unit = src('ticket-portal-unit.ts');
    expect(unit).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(unit).toContain('LOCK TABLE command_receipts,command_assertions,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE');
    expect(src('eligibility-projection-sources.ts')).toContain('supportEligibleFlagSql');
    expect(src('ticket-portal-image-sql.ts')).not.toContain('portalBoundImageSql');
    expect(src('ticket-portal-sources.ts')).toContain('SUPPORT_RD_TICKET_PORTAL_VALID_BINDINGS');
  });

  test('retirement:ts-statements is 18 after the customer purchase-scope fragment cut', () => {
    expect(totalSelects()).toBe(18);
    expect(totalSelects()).toBeLessThan(32);
  });
});
