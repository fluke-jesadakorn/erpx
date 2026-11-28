import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { beforeAll, describe, expect, test, vi } from 'vitest';

const { fragments } = vi.hoisted(() => ({ fragments: { full: '', core: '' } }));

vi.mock('server-only', () => ({}));
vi.mock('@erpx/support-server/adapters/native-support-read', async (importOriginal) => {
  const original = await importOriginal<typeof import('../../../modules/support/server/src/adapters/native-support-read')>();
  return {
    ...original,
    nativeSupportReadFragment: async (kind: string) =>
      kind.endsWith('_full') ? fragments.full : fragments.core,
  };
});

import { readTailR43ColumnsFromSql } from '../../../modules/support/server/src/adapters/read-tail-r43-sql';
import { portalImageSelectedCte } from '../../../modules/support/server/src/adapters/ticket-portal-image-sql';
import { socialPmsSourceFacts } from '@erpx/pms-server/public/social-source-facts';
import {
  supportTicketPortalCoreImage,
  supportTicketPortalImage,
} from '@erpx/support-server/adapters/ticket-portal-sources';

const srcRoot = resolve(__dirname, '../../../modules/support/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const rustLiteral = (name: string): string => {
  const match = src('native_support_read_sql.rs').match(new RegExp(`const SQL_${name}: &str =\\s*("(?:[^"\\\\]|\\\\.)*");`));
  if (!match) throw new Error(`missing SQL_${name}`);
  return JSON.parse(match[1]!) as string;
};
const totalSelects = () => {
  let total = 0;
  for (const rel of readdirSync(srcRoot, { recursive: true })) {
    if (!String(rel).endsWith('.ts')) continue;
    total += readFileSync(resolve(srcRoot, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
  }
  return total;
};

/** Byte-identical HEAD oracles captured before the cutover. Expected SQL is
 * written out (never derived from the renderer), so a head, column, bind,
 * whitespace or trailing-clause regression fails the oracle. The r46 slice
 * moves only the `SELECT * FROM ${body}` head of the selected-bindings CTE
 * through the r43 column-projection renderer; the WITH wrapper, bind list,
 * image aggregates, locks and native read kinds stay at the call sites. */
const cteTwo = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?))`;
const cteOne = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?))`;
const cteZero = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN ())`;
const full =
  `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?)) SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),(SELECT jsonb_agg(to_jsonb(identity) ORDER BY identity.binding_id) FROM support_ticket_portal_identities identity WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=identity.binding_id)),(SELECT jsonb_agg(to_jsonb(read) ORDER BY read.id) FROM support_ticket_portal_reads read WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=read.binding_id)),(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)),(SELECT jsonb_agg(to_jsonb(ack) ORDER BY ack.id) FROM support_ticket_portal_acknowledgements ack WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=ack.binding_id)))::text image`;
const core =
  `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?)) SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)))::text image`;

beforeAll(() => {
  fragments.full = full.slice(full.indexOf(') SELECT jsonb_build_array') + 2);
  fragments.core = core.slice(core.indexOf(') SELECT jsonb_build_array') + 2);
});

const oracles = [
  { name: 'two-binding selected CTE', rendered: portalImageSelectedCte(['b1', 'b2']), sql: cteTwo, sha: '21fc4de025dc2371cf53d882851e8825b1e0a894c86402c34437455138f946d0', binds: 2 },
  { name: 'one-binding selected CTE', rendered: portalImageSelectedCte(['b1']), sql: cteOne, sha: '0166d29508616c329564510ea04628eb0e6a444a5124bdf322396fdc41d07655', binds: 1 },
  { name: 'empty-binding selected CTE', rendered: portalImageSelectedCte([]), sql: cteZero, sha: '5709c71334c4d6a0d22bd68ead8b36bc30fb259ebd4298ec2d937813de8b9c91', binds: 0 },
] as const;

/** Capture the exact SQL each reader prepares without executing it. */
function capture() {
  return {
    prepare(sql: string) {
      return { bind: () => ({ first: async () => sql }) };
    },
  } as never;
}

describe('support read-tail r46 byte-identical selected-bindings column-projection head', () => {
  test.each(oracles)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql, sha, binds }) => {
    expect(rendered).toBe(sql);
    expect(Buffer.byteLength(rendered)).toBe(Buffer.byteLength(sql));
    expect(sha256(rendered)).toBe(sha);
    expect(sha256(rendered)).toBe(sha256(sql));
    expect(rendered.match(/\?/g) ?? []).toHaveLength(binds);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('full and core portal image readers stay byte-identical end to end', async () => {
    const fullSql = (await supportTicketPortalImage(capture(), ['b1', 'b2']))!;
    const coreSql = (await supportTicketPortalCoreImage(capture(), ['b1', 'b2']))!;
    expect(fullSql).toBe(full);
    expect(Buffer.byteLength(fullSql)).toBe(1262);
    expect(sha256(fullSql)).toBe('966e36c9361810920d381b6374461c35d6083d846b862bef7633826f44546382');
    expect(coreSql).toBe(core);
    expect(Buffer.byteLength(coreSql)).toBe(722);
    expect(sha256(coreSql)).toBe('512bc190ba0f62571af4abc501c1e31e95f12ad256d198029303a87948ccd00e');
    expect(fullSql.match(/\?/g) ?? []).toHaveLength(2);
    expect(coreSql.match(/\?/g) ?? []).toHaveLength(2);
    expect(fullSql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(coreSql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the r43 renderer owns the one shared read token and the CTE delegates to it', () => {
    const helper = src('read-tail-r43-sql.ts');
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper.match(/readTailR43ColumnsFromSql =/g)).toHaveLength(1);
    expect(readTailR43ColumnsFromSql('x', 'y')).toBe('SELECT x FROM y');

    const images = src('ticket-portal-image-sql.ts');
    expect(images).toContain("from './read-tail-r43-sql'");
    expect(images).toContain("readTailR43ColumnsFromSql('*', `support_ticket_portal_bindings WHERE id IN (${ids.map(() => '?').join(',')})`)");
    expect(images).not.toContain('WITH selected AS(SELECT * FROM');
    expect(images).toContain('export const portalImageSelectedCte =');
    expect(images).not.toContain('portalBoundImageSql');
    expect(images).not.toContain('portalImageAggregateSql');
    expect(src('ticket-portal-sources.ts')).toContain('SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_FULL');
    expect(src('ticket-portal-sources.ts')).toContain('SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_CORE');
  });

  test('native registry kinds, locks, advisory and earlier r18-r45 helpers stay untouched', () => {
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
    expect(existsSync(resolve(srcRoot, 'adapters', 'read-tail-r44-sql.ts'))).toBe(false);
    expect(src('read-tail-r45-sql.ts')).toContain('readTailR45AssignmentExistsSql =');
    expect(src('eligibility-projection-sources.ts')).toContain('supportEligibleFlagSql');
  });

  test('r48 retires the PMS attachment source-image read to the native owner', () => {
    const attachment = src('attachment-read.ts');
    expect(attachment).not.toContain('SELECT jsonb_agg(to_jsonb(source)');
    expect(attachment).not.toContain('socialPmsSourceFacts');
    expect(attachment).toContain('SUPPORT_RD_ATTACHMENT_PMS_SOURCE_IMAGE');
    expect(attachment).toContain('nativeSupportReadFirst<{image:string}>(SUPPORT_RD_ATTACHMENT_PMS_SOURCE_IMAGE,[source.id,companyId],database)');
    const bridge = src('native-support-read.ts');
    expect(bridge).toContain("SUPPORT_RD_ATTACHMENT_PMS_SOURCE_IMAGE = 'support_rd_attachment_pms_source_image'");
    expect(bridge).toContain('[SUPPORT_RD_ATTACHMENT_PMS_SOURCE_IMAGE]: 2');
    const registry = src('native_support_read_sql.rs');
    expect(registry).toContain('pub const RD_ATTACHMENT_PMS_SOURCE_IMAGE: &str = "support_rd_attachment_pms_source_image";');
    expect(registry).toContain('(RD_ATTACHMENT_PMS_SOURCE_IMAGE, SQL_ATTACHMENT_PMS_SOURCE_IMAGE, 2usize)');
    expect(registry).toContain('assert_eq!(STATEMENTS.len(), 55);');
    const expected = `SELECT jsonb_agg(to_jsonb(source) ORDER BY source.kind)::text image FROM (${socialPmsSourceFacts}) source WHERE source.id=? AND source.company_id=?`;
    expect(rustLiteral('ATTACHMENT_PMS_SOURCE_IMAGE')).toBe(expected);
    expect((expected.match(/\?/g) ?? [])).toHaveLength(2);
    expect(expected).not.toMatch(/\bINSERT\b|\bUPDATE\b|\bDELETE\b|FOR UPDATE|FOR SHARE|LOCK TABLE/i);
  });

  test('retirement:ts-statements is 18 after the customer purchase-scope fragment cut', () => {
    expect(totalSelects()).toBe(18);
    expect(totalSelects()).toBeLessThan(24);
  });
});
