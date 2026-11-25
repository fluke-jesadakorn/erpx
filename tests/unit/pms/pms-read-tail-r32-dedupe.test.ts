import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { pmsBookingAllocationRefundSql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r32-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r32-sql.ts';
const CALL_SITE = 'adapters/pms-booking-changes.ts';

const CORE = "SELECT a.*,COALESCE((SELECT SUM((part->>'amount')::bigint) FROM pms_booking_refund_requests r CROSS JOIN LATERAL jsonb_array_elements(r.allocations_json::jsonb) part WHERE r.payment_id=a.payment_id AND part->>'entryId'=a.entry_id),0) refunded FROM pms_booking_allocations a WHERE a.stay_id=?";

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r32 parameterized family dedupe', () => {
  it('declares the shared fragment exactly once', async () => {
    expect((await src(SHARED)).split('pmsBookingAllocationRefundSql').length - 1).toBe(1);
  });

  it('generates byte-identical SQL for both closed variants', () => {
    expect(pmsBookingAllocationRefundSql(false)).toBe(CORE);
    expect(pmsBookingAllocationRefundSql(true)).toBe(`${CORE} ORDER BY a.created_at,a.id`);
  });

  it('routes both call sites through the shared fragment and removes the inline literals', async () => {
    const text = await src(CALL_SITE);
    expect(text.split('pmsBookingAllocationRefundSql').length - 1).toBe(3); // import + 2 uses
    expect(text).not.toContain(CORE);
    expect(text).not.toContain(' ORDER BY a.created_at,a.id');
  });

  it('keeps the fragment read-only and leaves locks, r17-r31 fragments and the registry untouched', async () => {
    const shared = await src(SHARED);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const booking = await src(CALL_SITE);
    expect(booking).toContain("'SELECT * FROM pms_booking_changes WHERE id=? FOR UPDATE'");
    expect(booking).toContain('from "./pms-read-tail-r17-sql"');
    expect(booking).not.toContain('pms-read-tail-r31-sql');
  });

  it('lowers the pms owner SELECT KPI strictly below the r31 after=347 baseline', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(220);
    expect(bare).toBe(221);
    expect(spaced).toBeLessThan(347);
  });
});
