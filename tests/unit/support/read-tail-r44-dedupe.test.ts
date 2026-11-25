import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { readTailR44ImageAggregateSql } from '../../../packages/server-support/src/adapters/read-tail-r44-sql';
import {
  portalBindingImageSql,
  portalBoundImageSql,
} from '../../../packages/server-support/src/adapters/ticket-portal-image-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
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
 * written out, never derived from the renderer, so a head, alias, ordering,
 * trailing FROM body, bind or whitespace regression fails the oracle. The
 * renderer moves only the shared `(SELECT jsonb_agg(to_jsonb(...) ORDER BY
 * ...) FROM ...)` head; each caller keeps its own binds, nested EXISTS, locks
 * and native read kind. */
const oracles = [
  {
    name: 'binding image',
    rendered: portalBindingImageSql(),
    sql: `(SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding)`,
    binds: 0,
  },
  {
    name: 'ticket bound-row image',
    rendered: portalBoundImageSql('ticket', 'support_tickets', 'id', 'binding.ticket_id=ticket.id'),
    sql: `(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id))`,
    binds: 0,
  },
  {
    name: 'event bound-row image',
    rendered: portalBoundImageSql('event', 'support_ticket_portal_events', 'id', 'binding.id=event.binding_id'),
    sql: `(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id))`,
    binds: 0,
  },
  {
    name: 'bound-row image with caller bind',
    rendered: portalBoundImageSql('read', 'support_ticket_portal_reads', 'id', 'binding.id=read.id AND read.company_id=?'),
    sql: `(SELECT jsonb_agg(to_jsonb(read) ORDER BY read.id) FROM support_ticket_portal_reads read WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=read.id AND read.company_id=?))`,
    binds: 1,
  },
] as const;

describe('support read-tail r44 byte-identical bound-row image aggregate head', () => {
  test.each(oracles)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql, binds }) => {
    expect(rendered).toBe(sql);
    expect(Buffer.byteLength(rendered)).toBe(Buffer.byteLength(sql));
    expect(sha256(rendered)).toBe(sha256(sql));
    expect(rendered.match(/\?/g) ?? []).toHaveLength(binds);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the owner renderer owns exactly the one shared read token', () => {
    const helper = src('read-tail-r44-sql.ts');
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper.match(/readTailR44ImageAggregateSql =/g)).toHaveLength(1);
    expect(readTailR44ImageAggregateSql('x', 'x.id', 't')).toBe('(SELECT jsonb_agg(to_jsonb(x) ORDER BY x.id) FROM t)');
  });

  test('both consumer sites route their former inline head through the renderer', () => {
    const images = src('ticket-portal-image-sql.ts');
    expect(images).toContain("from './read-tail-r44-sql'");
    expect(images).toContain("readTailR44ImageAggregateSql('binding', 'binding.id', 'selected binding')");
    expect(images).toContain('readTailR44ImageAggregateSql(alias, `${alias}.${orderBy}`, `${table} ${alias} WHERE EXISTS(SELECT 1 FROM selected binding WHERE ${predicate})`)');
    expect(images).not.toContain('jsonb_agg(to_jsonb(');
    expect(images).toContain('export const portalBoundImageSql =');
    expect(images).toContain('export const portalImageAggregateSql =');
  });

  test('native registry kinds, locks, advisory, eligibility wrappers and r18-r43 helpers stay untouched', () => {
    const registry = src('native_support_read_sql.rs');
    const bridge = src('native-support-read.ts');
    expect(registry).toContain('assert_eq!(STATEMENTS.len(), 50);');
    expect(registry).not.toMatch(/LOCK TABLE|pg_advisory_xact_lock/);
    expect(bridge).toContain('SUPPORT_RD_TICKET_WORK');
    expect(bridge).toContain('SUPPORT_RD_INCIDENT_WORK');
    const ticket = src('support-ticket-read.ts');
    expect(ticket).toContain('SELECT id FROM support_tickets WHERE id=? AND company_id=? FOR SHARE');
    const unit = src('ticket-portal-unit.ts');
    expect(unit).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(unit).toContain('LOCK TABLE command_receipts,command_assertions,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE');
    expect(src('eligibility-projection-sources.ts')).toContain('supportEligibleFlagSql');
    expect(src('ticket-portal-sources.ts')).toContain('export const portalValidBindingSelect=');
    expect(src('service-access.ts')).toContain('const activeAssignmentSql =');
    expect(src('read-tail-r43-sql.ts')).toContain('readTailR43ColumnsFromSql =');
  });

  test('retirement:ts-statements drops from the r43 AFTER=33 to 32', () => {
    expect(totalSelects()).toBe(32);
    expect(totalSelects()).toBeLessThan(33);
  });
});
