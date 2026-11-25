import { existsSync } from 'node:fs';
import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const count = (text: string, needle: string) => text.split(needle).length - 1;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

/** The six exports retired at r52, with the exact bytes each one carried at HEAD (KPI 252). */
const RETIRED = [
  {
    symbol: 'PMS_SCOPED_RECORDS_SQL',
    sql: 'SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q',
    wholeFile: true,
  },
  {
    symbol: 'PMS_GUEST_ACCOUNT_ID_MEMBER_SQL',
    sql: "AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)",
  },
  {
    symbol: 'PMS_GUEST_ACCOUNT_CASHIER_MEMBER_SQL',
    sql: "WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id IN (SELECT id FROM account_ids)",
  },
  {
    symbol: 'PMS_RESERVATION_ID_MEMBER_SQL',
    sql: "AND r.data_json::jsonb->>'reservationId' IN (SELECT id FROM booking_ids)",
  },
  {
    symbol: 'PMS_GUEST_ID_FROM_ACCOUNTS_SQL',
    sql: "SELECT data_json::jsonb->>'guestId' FROM accounts",
  },
  {
    symbol: 'PMS_GUEST_ID_FROM_STAYS_SQL',
    sql: "SELECT data_json::jsonb->>'guestId' FROM stays",
  },
] as const;

describe('pms read-tail r52 dead-export retirement', () => {
  it('deletes the r23 module and leaves no live import, export or SQL literal', async () => {
    expect(existsSync(new URL('adapters/pms-read-tail-r23-sql.ts', dir))).toBe(false);
    const all = (await Promise.all((await tsFiles(dir)).map(file => readFile(file, 'utf8')))).join('\n');
    expect(all).not.toContain('pms-read-tail-r23-sql');
    for (const { symbol, sql } of RETIRED) {
      expect(count(all, symbol), symbol).toBe(0);
      expect(count(all, sql), sql).toBe(0);
    }
  });

  it('pins the surviving r25/r26 helpers byte-for-byte', async () => {
    const r25 = await src('adapters/pms-read-tail-r25-sql.ts');
    const r26 = await src('adapters/pms-read-tail-r26-sql.ts');
    expect(count(r25, 'export const PMS_GUEST_STAYS_CHOSEN_MEMBER_SQL =')).toBe(1);
    expect(count(r26, 'export const PMS_IDS_JSONB_MEMBER_SQL =')).toBe(1);
    expect(count(r26, 'export const PMS_CHOSEN_RECORDS_SQL =')).toBe(1);
    expect(r25).toContain('export const PMS_GUEST_STAYS_CHOSEN_MEMBER_SQL = "OR (kind=\'guest\' AND id IN (SELECT data->>\'guestId\' FROM stays UNION SELECT data->>\'guestId\' FROM chosen))";');
    expect(r26).toContain('export const PMS_IDS_JSONB_MEMBER_SQL = "(SELECT jsonb_array_elements_text(?::jsonb))";');
    expect(r26).toContain('export const PMS_CHOSEN_RECORDS_SQL = `SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id IN ${PMS_IDS_JSONB_MEMBER_SQL}`;');
    expect(count(r25, 'SELECT ')).toBe(2);
    expect(count(r26, 'SELECT ')).toBe(2);
    for (const [name, text] of [['r25', r25], ['r26', r26]] as const) {
      const statements = text.split('\n').filter(line => line.trimStart().startsWith('export const ')).join('\n');
      expect(statements, `${name} executable statements`).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('retains every other r17-r51 SQL helper module', async () => {
    const rounds = [
      17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
      41, 42, 43, 44, 45, 46, 49, 50, 51,
    ];
    expect(rounds).not.toContain(23);
    for (const round of rounds) {
      expect((await src(`adapters/pms-read-tail-r${round}-sql.ts`)).length, `r${round}`).toBeGreaterThan(0);
    }
  });

  it('leaves row locks, advisory locks and the native read registry untouched', async () => {
    expect(await src('adapters/pms.ts')).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    const rust = await src('adapters/native_read_sql.rs');
    expect(rust).toContain('const SQL_STAY_COMMAND: &str = "WITH scope AS (');
    expect(rust).toContain('pub const PMS_RD_STAY_COMMAND: &str = "pms_rd_stay_command";');
  });

  it('lowers the pms owner SELECT KPI from 252 to 246', async () => {
    let spaced = 0;
    let bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += count(text, 'SELECT ');
      bare += count(text, 'SELECT');
    }
    expect(spaced).toBe(242);
    expect(bare).toBe(244);
    expect(spaced).toBeLessThan(252);
    expect(bare).toBeLessThan(254);
  });
});
