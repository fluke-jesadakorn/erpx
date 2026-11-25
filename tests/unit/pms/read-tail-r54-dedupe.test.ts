import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { pmsBookingPaymentIdProbeSql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r29-sql';
import { PMS_BOOKING_PAYMENT_ID_HEAD_SQL } from '../../../packages/server-pms/src/adapters/pms-read-tail-r37-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const count = (text: string, needle: string) => text.split(needle).length - 1;

// The byte-identical owner-local booking-payment id head retired at HEAD (KPI 245).
// The r29 parameterized point probe and the r37 shared renderer opened with these
// exact bytes; every recomposed probe must reproduce its original SQL verbatim.
const HEAD = 'SELECT id FROM pms_booking_payments WHERE';
const HEAD_SHA = 'f5cd09b060e501bed659c8c396af397d3eb34b264ccd950c607e9cc92a0a110b';

const PROBES = [
  {
    column: 'property_id',
    status: 'payment_pending',
    sql: "SELECT id FROM pms_booking_payments WHERE property_id=? AND status='payment_pending' LIMIT 1",
    sha: 'bd6259dafe4028fcf148e222ebb8a11bc7955838f26aa754fd334a5bf8b7d5a3',
  },
  {
    column: 'reservation_id',
    status: 'payment_pending',
    sql: "SELECT id FROM pms_booking_payments WHERE reservation_id=? AND status='payment_pending' LIMIT 1",
    sha: '3572cc1f6a5636467c18a122bfa23440dad44680267c38c1b5fbc0a48973e191',
  },
  {
    column: 'reservation_id',
    status: 'confirmed',
    sql: "SELECT id FROM pms_booking_payments WHERE reservation_id=? AND status='confirmed' LIMIT 1",
    sha: '845d9285ff33ea816aa0dd72f1bde82683abdac7a2e70785bc7ba30e8d069995',
  },
] as const;

const RENDERER = 'adapters/pms-read-tail-r37-sql.ts';
const R29 = 'adapters/pms-read-tail-r29-sql.ts';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r54 owner-local booking-payment head dedupe', () => {
  it('pins the exact HEAD bytes of the shared booking-payment head', () => {
    expect(sha256(HEAD), 'head sha').toBe(HEAD_SHA);
    expect(HEAD.length, 'head length').toBe(41);
    expect(count(HEAD, 'SELECT '), 'head selects').toBe(1);
    expect(count(HEAD, '?'), 'head binds').toBe(0);
    expect(PMS_BOOKING_PAYMENT_ID_HEAD_SQL).toBe(HEAD);
    expect(sha256(PMS_BOOKING_PAYMENT_ID_HEAD_SQL)).toBe(HEAD_SHA);
  });

  it('renders all three point probes byte-identically with one bind each', () => {
    for (const probe of PROBES) {
      const rendered = pmsBookingPaymentIdProbeSql(probe.column, probe.status);
      expect(rendered, probe.sql).toBe(probe.sql);
      expect(sha256(rendered), probe.sql).toBe(probe.sha);
      expect(count(rendered, 'SELECT ')).toBe(1);
      expect(count(rendered, '?'), 'bind order').toBe(1);
      expect(rendered.startsWith(`${HEAD} `)).toBe(true);
    }
  });

  it('declares the head exactly once in the owner-local renderer with no write/lock seam', async () => {
    const shared = await src(RENDERER);
    expect(count(shared, HEAD)).toBe(1);
    expect(count(shared, 'PMS_BOOKING_PAYMENT_ID_HEAD_SQL')).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('routes the r29 probe through the renderer and retires the inline head', async () => {
    const r29 = await src(R29);
    expect(count(r29, 'PMS_BOOKING_PAYMENT_ID_HEAD_SQL')).toBe(2); // import + 1 use
    expect(r29).toContain("import { PMS_BOOKING_PAYMENT_ID_HEAD_SQL } from './pms-read-tail-r37-sql'");
    expect(r29).toContain('${PMS_BOOKING_PAYMENT_ID_HEAD_SQL} ${column}=?');
    expect(count(r29, HEAD), 'r29 inline head retired').toBe(0);
    // Distinct fragments, wrappers and bind order stay byte-identical at the r29 site.
    expect(r29).toContain("export const PMS_POSTINGS_PROJECTION_SQL = pmsOwnerProjectionHeadSql('postings');");
    expect(r29).toContain('export const PMS_PROPERTIES_TEN_COL_JOIN_SQL = "JOIN (SELECT');
    expect(r29).toContain("status: 'payment_pending' | 'confirmed',");
  });

  it('leaves locks, native registry, cross-owner families and prior helpers untouched', async () => {
    expect(await src('adapters/pms.ts')).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    const rust = await src('adapters/native_read_sql.rs');
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(45);
    expect(rust).toContain('const SQL_STAY_COMMAND: &str = "WITH scope AS (');
    const bridge = await src('adapters/native-pms-read.ts');
    expect((bridge.match(/^export const PMS_RD_/gm) ?? []).length).toBe(45);
    // The pms_properties family has cross-owner consumers and stays inline (r53 deferred it).
    expect(await src('adapters/pms-read-tail-r20-sql.ts')).toContain('export const PMS_PROPERTIES_PROJECTION_SQL = "SELECT');
    expect(await src(R29)).toContain('export const PMS_PROPERTIES_TEN_COL_JOIN_SQL = "JOIN (SELECT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 49, 50, 51, 53]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length, `r${n}`).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 245 to 244', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += count(text, 'SELECT ');
      bare += count(text, 'SELECT');
    }
    expect(spaced).toBe(224);
    expect(bare).toBe(225);
    expect(spaced).toBeLessThan(245);
    expect(bare).toBeLessThan(247);
  });
});
