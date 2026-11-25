import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { readTailR45AssignmentExistsSql } from '../../../packages/server-support/src/adapters/read-tail-r45-sql';
import {
  supportEligibleFlagSql,
  supportAssignmentAccessSql,
} from '../../../packages/server-support/src/adapters/eligibility-projection-sources';

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
 * written out (with the retained HEAD assignment projection inlined), never
 * derived from the renderer, so a head, alias, WHERE body, bind or whitespace
 * regression fails the oracle. The renderer moves only the shared
 * `EXISTS(SELECT 1 FROM (assignments) alias WHERE ...)` head; each witness keeps
 * its own predicate, trailing clause, binds and native read kind. */
const assignments = 'SELECT "id","user_id","role","modules_json","company_id","branch_id","valid_from","valid_until","revoked_at","version","scope_kind","scope_node_id","parent_id","source","source_id" FROM assignments';
const eligibilitySql = `CASE WHEN EXISTS(SELECT 1 FROM (${assignments}) m WHERE t.status=1 AND m.user_id=?) THEN 1 ELSE 0 END eligible`;
const accessSql = `EXISTS(SELECT 1 FROM (${assignments}) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id\n\n  AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\ng.role='support.read')`;

const oracles = [
  {
    name: 'eligibility flag witness',
    rendered: supportEligibleFlagSql('t.status=1', 'eligible'),
    sql: eligibilitySql,
    sha: 'ef73ccaa4fb1123a5fb028aaa892e060dda2c3d32d2d04260b5f426017074b03',
    binds: 1,
  },
  {
    name: 'assignment access witness',
    rendered: supportAssignmentAccessSql('t', '\n  ', "g.role='support.read'"),
    sql: accessSql,
    sha: 'e0e5251a473228741d781edcd7ddf0a04e44f6b8ac0ff8f580bb17ef0eb88b15',
    binds: 3,
  },
] as const;

describe('support read-tail r45 byte-identical active-assignment EXISTS head', () => {
  test.each(oracles)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql, sha, binds }) => {
    expect(rendered).toBe(sql);
    expect(Buffer.byteLength(rendered)).toBe(Buffer.byteLength(sql));
    expect(sha256(rendered)).toBe(sha);
    expect(sha256(rendered)).toBe(sha256(sql));
    expect(rendered.match(/\?/g) ?? []).toHaveLength(binds);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the owner renderer owns exactly the one shared read token', () => {
    const helper = src('read-tail-r45-sql.ts');
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper.match(/readTailR45AssignmentExistsSql =/g)).toHaveLength(1);
    expect(readTailR45AssignmentExistsSql('x', 'x.id=?')).toBe(
      `EXISTS(SELECT 1 FROM (${assignments}) x WHERE x.id=?)`,
    );
  });

  test('both consumer witnesses route their former inline head through the renderer', () => {
    const elig = src('eligibility-projection-sources.ts');
    expect(elig).toContain("from './read-tail-r45-sql'");
    expect(elig).toContain("readTailR45AssignmentExistsSql('m', `${predicate} AND m.user_id=?`)");
    expect(elig).toContain("readTailR45AssignmentExistsSql('g', `g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id");
    expect(elig).not.toContain('EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m');
    expect(elig).not.toContain('EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) g');
    expect(elig).toContain('export const supportReviewerCountSql =');
  });

  test('native registry kinds, locks, advisory and earlier r18-r44 helpers stay untouched', () => {
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
    expect(src('read-tail-r43-sql.ts')).toContain('readTailR43ColumnsFromSql =');
    expect(src('read-tail-r44-sql.ts')).toContain('readTailR44ImageAggregateSql =');
  });

  test('retirement:ts-statements drops from the r44 AFTER=32 to 31', () => {
    expect(totalSelects()).toBe(31);
    expect(totalSelects()).toBeLessThan(32);
  });
});
