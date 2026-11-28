import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../modules/support/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const totalSelects = () => {
  let total = 0;
  for (const rel of readdirSync(srcRoot, { recursive: true })) {
    if (!String(rel).endsWith('.ts')) continue;
    total += readFileSync(resolve(srcRoot, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
  }
  return total;
};

describe('Support read-tail r44 bound-row image aggregate retirement', () => {
  test('the retired renderer file is gone and no source imports it', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/read-tail-r44-sql.ts'))).toBe(false);
    for (const name of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (!name.endsWith('.ts')) continue;
      const text = readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
      expect(text, name).not.toContain('read-tail-r44-sql');
      expect(text, name).not.toContain('readTailR44ImageAggregateSql');
    }
  });

  test('the image aggregate is now a fixed 0-bind native fragment', () => {
    const registry = src('native_support_read_sql.rs');
    expect(registry).toContain('pub const RD_TICKET_PORTAL_IMAGE_AGGREGATE_FULL: &str = "support_rd_ticket_portal_image_aggregate_full";');
    expect(registry).toContain('pub const RD_TICKET_PORTAL_IMAGE_AGGREGATE_CORE: &str = "support_rd_ticket_portal_image_aggregate_core";');
    expect(registry).toContain('tail_bind_counts: &[0]');
    expect(registry).toContain('const FRAGMENTS: &[NativeSupportReadFragment] = &[');
    const images = src('ticket-portal-image-sql.ts');
    expect(images).toContain('export const portalImageSelectedCte =');
    expect(images).not.toContain('jsonb_agg(to_jsonb(');
    expect(images).not.toContain('portalBoundImageSql');
    expect(images).not.toContain('portalImageAggregateSql');
    const sources = src('ticket-portal-sources.ts');
    expect(sources).toContain('SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_FULL');
    expect(sources).toContain('SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_CORE');
  });

  test('locks, advisory, eligibility wrappers and r18-r43 helpers stay untouched', () => {
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
    expect(src('service-access.ts')).toContain('SUPPORT_RD_SERVICE_MEMBER');
    expect(src('read-tail-r43-sql.ts')).toContain('readTailR43ColumnsFromSql =');
  });

  test('retirement:ts-statements drops from the r11 AFTER=24 to 18', () => {
    expect(totalSelects()).toBe(18);
    expect(totalSelects()).toBeLessThan(24);
  });
});
