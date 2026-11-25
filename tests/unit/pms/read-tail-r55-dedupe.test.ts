import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { PMS_GUEST_ACCOUNT_PROJECTION_HEAD_SQL } from '../../../packages/server-pms/src/adapters/pms-read-tail-r55-sql';
import { PMS_IDS_JSONB_MEMBER_SQL } from '../../../packages/server-pms/src/adapters/pms-read-tail-r26-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const count = (text: string, needle: string) => text.split(needle).length - 1;

// The byte-identical owner-local head retired at HEAD (KPI 244). Both point
// reads opened with these exact bytes and diverged only in their closed tails.
const HEAD = 'SELECT id,guest_account_id,';
const HEAD_SHA = 'eebea6f189b78fb02edb08a680972642fddc3f6610048142803f69133bf1e018';
const TAILS = [
  {
    name: 'pms_documents guest-account point read',
    tail: `kind,created_at FROM pms_documents WHERE property_id=? AND (?::jsonb IS NULL OR guest_account_id IN ${PMS_IDS_JSONB_MEMBER_SQL}) ORDER BY created_at DESC,id LIMIT 200`,
    sha: 'f0b917be21d1bcf2b1a38bd2a3b088c4bb617e2d9ebcaeb1c4556f2d7f9dffdc',
    selects: 2,
    binds: 3,
  },
  {
    name: 'pms_stripe_payments point read',
    tail: 'state,kind,amount_minor,refund_state,payer_email FROM pms_stripe_payments WHERE property_id=? ORDER BY created_at DESC LIMIT 100',
    sha: 'f6328955f434978321798c08a782a6dda1287de8ecf18d9b5080140f20535fd0',
    selects: 1,
    binds: 1,
  },
] as const;

const RENDERER = 'adapters/pms-read-tail-r55-sql.ts';
const PMS = 'adapters/pms.ts';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r55 owner-local guest-account head dedupe', () => {
  it('pins the exact HEAD bytes of the shared guest-account head', () => {
    expect(sha256(HEAD), 'head sha').toBe(HEAD_SHA);
    expect(HEAD.length, 'head length').toBe(27);
    expect(count(HEAD, 'SELECT '), 'head selects').toBe(1);
    expect(count(HEAD, '?'), 'head binds').toBe(0);
    expect(PMS_GUEST_ACCOUNT_PROJECTION_HEAD_SQL).toBe(HEAD);
    expect(sha256(PMS_GUEST_ACCOUNT_PROJECTION_HEAD_SQL)).toBe(HEAD_SHA);
  });

  it('recomposes both point reads byte-identically with unchanged selects and binds', () => {
    for (const o of TAILS) {
      const rendered = PMS_GUEST_ACCOUNT_PROJECTION_HEAD_SQL + o.tail;
      expect(sha256(rendered), o.name).toBe(o.sha);
      expect(rendered.startsWith(HEAD), o.name).toBe(true);
      expect(count(rendered, 'SELECT '), o.name).toBe(o.selects);
      expect(count(rendered, '?'), o.name).toBe(o.binds);
    }
  });

  it('declares the head exactly once in the owner-local renderer with no write/lock seam', async () => {
    const shared = await src(RENDERER);
    expect(count(shared, HEAD)).toBe(1);
    expect(count(shared, 'SELECT ')).toBe(1);
    expect(count(shared, 'PMS_GUEST_ACCOUNT_PROJECTION_HEAD_SQL')).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('routes both sites through the renderer and retires the inline heads', async () => {
    const pms = await src(PMS);
    expect(count(pms, 'PMS_GUEST_ACCOUNT_PROJECTION_HEAD_SQL')).toBe(3); // import + 2 uses
    expect(pms).toContain('import { PMS_GUEST_ACCOUNT_PROJECTION_HEAD_SQL } from "./pms-read-tail-r55-sql";');
    expect(pms).toContain('${PMS_GUEST_ACCOUNT_PROJECTION_HEAD_SQL}kind,created_at FROM pms_documents');
    expect(pms).toContain('${PMS_GUEST_ACCOUNT_PROJECTION_HEAD_SQL}state,kind,amount_minor,refund_state,payer_email FROM pms_stripe_payments');
    expect(count(pms, HEAD), 'inline head retired').toBe(0);
  });

  it('leaves locks, native registry, cross-owner families and prior helpers untouched', async () => {
    expect(await src(PMS)).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    const rust = await src('adapters/native_read_sql.rs');
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(33);
    expect(rust).toContain('const SQL_STAY_COMMAND: &str = "WITH scope AS (');
    const bridge = await src('adapters/native-pms-read.ts');
    expect((bridge.match(/^export const PMS_RD_/gm) ?? []).length).toBe(33);
    // The pms_properties family has cross-owner consumers and stays inline.
    expect(await src('adapters/pms-read-tail-r20-sql.ts')).toContain('export const PMS_PROPERTIES_PROJECTION_SQL = "SELECT');
    expect(await src('adapters/pms-read-tail-r29-sql.ts')).toContain('export const PMS_PROPERTIES_TEN_COL_JOIN_SQL = "JOIN (SELECT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 49, 50, 51, 53, 55]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length, `r${n}`).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 244 to 243', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += count(text, 'SELECT ');
      bare += count(text, 'SELECT');
    }
    expect(spaced).toBe(237);
    expect(bare).toBe(238);
    expect(spaced).toBeLessThan(244);
    expect(bare).toBeLessThan(246);
  });
});
