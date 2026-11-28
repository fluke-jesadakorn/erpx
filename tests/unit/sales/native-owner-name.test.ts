import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { SALES_RD_OWNER_NAME_PROJECTION } from '../../../modules/sales/server/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../modules/sales/server/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = read('native_sales_read_sql.rs');
const shared = read('followup-source-sql.ts');
const followups = read('followups.ts');
const calendar = read('calendar-customer-followups-facts.ts');

describe('native Sales owner-name projection', () => {
  test('retired projection literal is byte-identical in the Rust registry', () => {
    const marker = 'const SQL_OWNER_NAME_PROJECTION: &str = "';
    const at = registry.indexOf(marker);
    expect(at).toBeGreaterThanOrEqual(0);
    const end = registry.indexOf('";', at + marker.length);
    expect(end).toBeGreaterThan(at);
    const sql = registry.slice(at + marker.length, end).replace(/\\"/g, '"');
    expect(sql).toHaveLength(347);
    expect(createHash('sha256').update(sql).digest('hex')).toBe(
      'b96e9c3bb47b4403ad7719ae87c607b816addf96bf1ceb598a135574f34774ae',
    );
    expect((sql.match(/\?/g) ?? []).length).toBe(0);
    expect(registry).toContain(`"${SALES_RD_OWNER_NAME_PROJECTION}"`);
    expect(registry).toContain('bind_count: 0,');
  });

  test('TS callers compose the planned fragment with no SQL mirror', () => {
    expect(shared).not.toContain('export const ownerNameProjection=');
    for (const source of [followups, calendar]) {
      expect(source).toContain('nativeSalesPlan(SALES_RD_OWNER_NAME_PROJECTION,[])');
      expect(source).toContain('${ownerName}');
      expect(source).not.toContain('${ownerNameProjection}');
    }
  });
});
