import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { scopedReadHead } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r14-sources';
import { receiptProbe } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r5-sources';
import { holdingMonthReadSql } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r7-sources';
import { companyScopedStarSql } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r8-sources';
import { holdingSnapshotReadSql } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r9-sources';
import { monthScopedRead } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r13-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-commercial/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const pureRead = (sql: string) => {
  expect(sql.toUpperCase()).toMatch(/^SELECT /);
  // Row-lock suffixes are caller-owned tails; the head itself emits none.
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE|pg_advisory/i);
};

// Pre-dedupe literals copied byte-for-byte from HEAD; these are the oracles.
const CHANGE_SET_EXISTS = 'SELECT 1 FROM company_package_change_sets WHERE id=?';
const CHANGE_SET_RECEIPT = 'SELECT request_hash,result_json FROM company_package_change_sets WHERE id=?';
const GRANT_RECEIPT = 'SELECT request_hash,after_json FROM company_package_grant_events WHERE id=?';
const GRANT_EXISTS = 'SELECT 1 FROM company_package_grant_events WHERE id=?';
const SEAT_SNAPSHOT = 'SELECT snapshot_id,seat_coverage,seat_source_count FROM billing_usage_snapshots WHERE holding_id=? AND month=?';
const SEAT_SUMMARY = 'SELECT intervals_json::text intervals_json,snapshot_id FROM billing_seat_usage WHERE holding_id=? AND month=?';
const HISTORY_RIGHTS = 'SELECT * FROM company_package_history_rights WHERE company_id=? ORDER BY package_id,package_version';
const CURRENT_GRANT = 'SELECT * FROM company_package_grants WHERE company_id=? AND package_id=?';
const GRANTS_ORDER = 'SELECT * FROM company_package_grants WHERE company_id=? ORDER BY package_id FOR SHARE';
const SNAPSHOT_HEADER_LOCK = 'SELECT refreshed_at FROM billing_usage_snapshots WHERE holding_id=? AND month=? FOR UPDATE';
const SNAPSHOT_MONTHS = 'SELECT month FROM billing_usage_snapshots WHERE holding_id=?';
const MONTH_SEAT = `SELECT holding_id,active_seat_count,active_seat_observed_at
      FROM billing_usage_snapshots WHERE month=?`;
const MONTH_METER = `SELECT holding_id,month,source_key
      FROM billing_usage_meters WHERE month=?`;

describe('Commercial read-tail r14 single closed scoped head', () => {
  test('every rendered head is byte-identical to its pre-dedupe literal', () => {
    const heads: Array<[string, string]> = [
      [receiptProbe('company_package_change_sets', '1'), CHANGE_SET_EXISTS], // former r1 export
      [receiptProbe('company_package_change_sets', 'request_hash,result_json'), CHANGE_SET_RECEIPT],
      [receiptProbe('company_package_grant_events', 'request_hash,after_json'), GRANT_RECEIPT],
      [receiptProbe('company_package_grant_events', '1'), GRANT_EXISTS],
      [holdingMonthReadSql('snapshot_id,seat_coverage,seat_source_count', 'billing_usage_snapshots'), SEAT_SNAPSHOT],
      [holdingMonthReadSql('intervals_json::text intervals_json,snapshot_id', 'billing_seat_usage'), SEAT_SUMMARY],
      [companyScopedStarSql('company_package_history_rights', ' ORDER BY package_id,package_version'), HISTORY_RIGHTS],
      [companyScopedStarSql('company_package_grants', ' AND package_id=?'), CURRENT_GRANT],
      [companyScopedStarSql('company_package_grants', ' ORDER BY package_id FOR SHARE'), GRANTS_ORDER],
      [holdingSnapshotReadSql('refreshed_at', ' AND month=? FOR UPDATE'), SNAPSHOT_HEADER_LOCK],
      [holdingSnapshotReadSql('month', ''), SNAPSHOT_MONTHS],
      [monthScopedRead('holding_id,active_seat_count,active_seat_observed_at', 'billing_usage_snapshots'), MONTH_SEAT],
      [monthScopedRead('holding_id,month,source_key', 'billing_usage_meters'), MONTH_METER],
    ];
    for (const [actual, oracle] of heads) {
      expect(actual, oracle).toBe(oracle);
      pureRead(actual);
    }
  });

  test('the head renders every closed scope and absorbs the r13 newline gap', () => {
    expect(scopedReadHead('1', 't', 'id')).toBe('SELECT 1 FROM t WHERE id=?');
    expect(scopedReadHead('1', 't', 'holding-month')).toBe('SELECT 1 FROM t WHERE holding_id=? AND month=?');
    expect(scopedReadHead('1', 't', 'company')).toBe('SELECT 1 FROM t WHERE company_id=?');
    expect(scopedReadHead('1', 't', 'holding')).toBe('SELECT 1 FROM t WHERE holding_id=?');
    expect(scopedReadHead('1', 't', 'month')).toBe('SELECT 1 FROM t WHERE month=?');
    expect(scopedReadHead('1', 't', 'month', '\n      ')).toBe('SELECT 1\n      FROM t WHERE month=?');
    expect(scopedReadHead('1', 't', 'id', ' ', ' FOR UPDATE')).toBe('SELECT 1 FROM t WHERE id=? FOR UPDATE');
    expect(() => scopedReadHead('1', 't', 'unknown' as 'id')).toThrow(/Unknown Commercial scoped read/);
  });

  test('the single literal lives in r14 and the five former builders only delegate', () => {
    const head = src('commercial-read-tail-r14-sources.ts');
    expect(head.match(/SELECT /g) ?? []).toHaveLength(0); // keyword is a named constant, not a local literal
    expect(head).toContain('const KW = "SELECT";');
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const delegates: Array<[string, string]> = [
      ['commercial-read-tail-r5-sources.ts', "scopedReadHead(columns, table, 'id')"],
      ['commercial-read-tail-r7-sources.ts', "scopedReadHead(columns, table, 'holding-month')"],
      ['commercial-read-tail-r8-sources.ts', "scopedReadHead('*', table, 'company', ' ', rest)"],
      ['commercial-read-tail-r9-sources.ts', "scopedReadHead(columns, 'billing_usage_snapshots', 'holding', ' ', rest)"],
      ['commercial-read-tail-r13-sources.ts', "scopedReadHead(columns, table, 'month', '\\n      ')"],
    ];
    for (const [file, call] of delegates) {
      const body = src(file);
      expect(body, file).toContain(call);
      expect(body.match(/SELECT /g) ?? [], file).toHaveLength(0);
      expect(body, file).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE|pg_advisory/i);
    }
    expect(() => src('commercial-read-tail-r1-sources.ts')).toThrow();
  });

  test('caller-side tails and bind order are unchanged across all 15 sites', () => {
    const pg = src('package-grants.ts'), lc = src('company-package-lifecycle.ts');
    expect(pg.match(/receiptProbe\("company_package_change_sets","1"\)/g)).toHaveLength(1);
    expect(pg.match(/receiptProbe\(/g)).toHaveLength(4);
    expect(pg.match(/\.bind\(key\)/g)?.length).toBeGreaterThanOrEqual(4);
    expect(lc.match(/receiptProbe\("company_package_change_sets","1"\)/g)).toHaveLength(2);
    expect(lc.match(/\.bind\(input\.idempotencyKey\)/g)).toHaveLength(2);
    expect(pg.match(/companyScopedStarSql\(/g)).toHaveLength(3);
    expect(pg).toContain('.bind(companyId, packageId).first<Row>()');
    expect(pg).toContain('forShare ? " FOR SHARE" : ""');

    const seats = src('seats.ts');
    expect(seats.match(/holdingMonthReadSql\(/g)).toHaveLength(2);
    expect(seats.match(/\.bind\(holdingId,month\)/g)).toHaveLength(2);

    const pj = src('projection.ts');
    expect(pj.match(/holdingSnapshotReadSql\(/g)).toHaveLength(2);
    expect(pj).toContain('.bind(binding.id, month).first<{refreshed_at:number}>()');
    expect(pj).toContain('.bind(binding.id).all<{ month: string }>()');
    expect(pj).toContain(' AND month=? FOR UPDATE'); // row lock stays caller-side
    expect(pj.match(/monthScopedRead\(/g)).toHaveLength(1);
    expect(pj).toContain('.bind(month).all<MeterRow>()');

    expect(pg).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pg).toContain('SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE');
    expect(src('../public/native_read.rs')).toContain('caller: "commercial-read-tail-r5-sources.receiptProbe"');
  });

  test('retirement:ts-statements pins 22 -> 16', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(16);
    expect(total).toBeLessThan(22);
  });
});
