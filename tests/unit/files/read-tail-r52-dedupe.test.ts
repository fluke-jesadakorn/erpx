import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { EVENT_ID_PROJECTION_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r52-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
// Fixed pre-r52 baseline: both inline audit-event id heads are still literal in
// this commit, so the rendered-SQL oracles stay valid after the slice is
// integrated and HEAD advances.
const BASE = 'a348ca7b11d99e3c7fe902d2d2a2a7e29167a7dc';
const HELPER = 'adapters/read-tail-r52-source-sql.ts';
const DENIED = 'adapters/attachment-denied-read-audits.ts';
const PREVIEW = 'adapters/customer-portal-files.ts';

const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${BASE}:packages/server-files/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

/** The single retired inline head and the two divergent tails. */
const HEAD = 'SELECT event.id,';
type Site = {
  alias: 'denied' | 'preview';
  tail: string;
  sql: string;
  sqlSha: string;
  bytes: number;
  binds: number;
};
const SITES: Site[] = [
  {
    alias: 'denied',
    tail: "event.reason,to_jsonb(event)::text witness FROM attachment_access_events event JOIN attachment_links link ON link.id=event.link_id AND link.attachment_id=event.attachment_id JOIN attachment_sets native_set ON native_set.id=link.set_id AND native_set.company_id=event.company_id WHERE event.company_id=? AND event.actor_id=? AND event.link_id=? AND event.outcome='denied' AND event.variant IN ('original','preview') ORDER BY event.id",
    sql: "SELECT event.id,event.reason,to_jsonb(event)::text witness FROM attachment_access_events event JOIN attachment_links link ON link.id=event.link_id AND link.attachment_id=event.attachment_id JOIN attachment_sets native_set ON native_set.id=link.set_id AND native_set.company_id=event.company_id WHERE event.company_id=? AND event.actor_id=? AND event.link_id=? AND event.outcome='denied' AND event.variant IN ('original','preview') ORDER BY event.id",
    sqlSha: '93441d3460552f582ba9199d296f32920056cc755dfb963c12161fce0b1556c3',
    bytes: 448,
    binds: 3,
  },
  {
    alias: 'preview',
    tail: "to_jsonb(event)::text image FROM attachment_access_events event WHERE event.link_id=? AND event.actor_id=? AND event.variant='preview' AND event.outcome='allowed' ORDER BY event.id",
    sql: "SELECT event.id,to_jsonb(event)::text image FROM attachment_access_events event WHERE event.link_id=? AND event.actor_id=? AND event.variant='preview' AND event.outcome='allowed' ORDER BY event.id",
    sqlSha: '25ce6656affd0adcbc3e3f7792707760f7c522a2dec2d014f560619cad8e1de0',
    bytes: 196,
    binds: 2,
  },
];

const lockRe = /INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const renderSql = (site: Site) => EVENT_ID_PROJECTION_HEAD_SQL + site.tail;
/** Render both files back to BASE: substitute each composition form, drop the import. */
const render = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r52-source-sql'))
  .join('\n')
  .replaceAll('${EVENT_ID_PROJECTION_HEAD_SQL}', HEAD)
  .replaceAll('EVENT_ID_PROJECTION_HEAD_SQL + "', '"' + HEAD);

describe('Files read-tail r52 per-site byte oracles', () => {
  test('the shared head is a 16-byte read-only projection', () => {
    expect(EVENT_ID_PROJECTION_HEAD_SQL).toBe(HEAD);
    expect(sha(HEAD)).toBe('6f47869b2f9aeabe91dd699ec8dc50e1faa133bdd178e9623ce840dfc4bcfaf4');
    expect(Buffer.byteLength(HEAD)).toBe(16);
    expect(count(HEAD, 'SELECT ')).toBe(1);
    expect(HEAD.startsWith('SELECT ')).toBe(true);
    expect(HEAD.endsWith('event.id,')).toBe(true);
    expect(HEAD).not.toMatch(lockRe);
  });

  for (const site of SITES) {
    test(`${site.alias} witness: head and tail render byte-exact`, () => {
      expect(renderSql(site)).toBe(site.sql);
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(Buffer.byteLength(site.sql)).toBe(site.bytes);
      expect(count(site.sql, 'SELECT ')).toBe(1);
      expect((site.sql.match(/\?/g) ?? []).length).toBe(site.binds);
      expect(count(site.sql, 'ORDER BY')).toBe(1);
      expect(site.sql).not.toMatch(lockRe);
      expect(site.sql.startsWith(HEAD)).toBe(true);
      expect(site.tail.startsWith(HEAD)).toBe(false);
      expect(site.tail.length).toBe(site.bytes - HEAD.length);
    });
  }

  test('the two heads were one byte-identical head with divergent tails', () => {
    expect(SITES[0].tail).not.toBe(SITES[1].tail);
    expect(SITES[0].tail).toContain('event.reason,');
    expect(SITES[1].tail).toContain('to_jsonb(event)::text image');
    expect(SITES[0].binds).not.toBe(SITES[1].binds);
    expect(SITES[0].bytes).not.toBe(SITES[1].bytes);
  });
});

describe('Files read-tail r52 audit-head fold', () => {
  test('one owner-local renderer replaces the two inline heads', () => {
    const helper = src(HELPER);
    expect(count(helper, 'export const EVENT_ID_PROJECTION_HEAD_SQL =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
    expect(helper.split('\n').find(l => l.startsWith('export const'))!).not.toMatch(lockRe);

    // No residual exact literal anywhere in the owner source except the helper.
    let residual = 0;
    for (const path of tsFiles(srcRoot)) residual += count(readFileSync(path, 'utf8'), HEAD);
    expect(residual).toBe(1);
  });

  test('each site keeps its own tail, binds and ORDER BY', () => {
    const denied = src(DENIED);
    expect(count(denied, '${EVENT_ID_PROJECTION_HEAD_SQL}')).toBe(1);
    expect(denied).toContain('${EVENT_ID_PROJECTION_HEAD_SQL}event.reason,to_jsonb(event)::text witness FROM attachment_access_events event');
    expect(denied).toContain('.bind(actor.companyId,actor.userId,linkId)');

    const preview = src(PREVIEW);
    expect(count(preview, 'EVENT_ID_PROJECTION_HEAD_SQL + "')).toBe(1);
    expect(preview).toContain('EVENT_ID_PROJECTION_HEAD_SQL + "to_jsonb(event)::text image FROM attachment_access_events event');
    expect(preview).toContain('.bind(linkId,actor.userId)');
  });

  test('rendered SQL is byte-identical to BASE; binds, locks, native and ORDER BY unchanged', () => {
    for (const rel of [DENIED, PREVIEW]) {
      const base = before(rel);
      const after = src(rel);
      expect(render(after)).toBe(base);
      for (const needle of [
        '.bind(', 'nativeWrite(', 'native', 'FOR UPDATE', 'FOR SHARE', 'FOR KEY SHARE',
        'LOCK TABLE', 'IN SHARE MODE', 'pg_advisory_xact_lock', 'ORDER BY',
      ]) {
        expect(count(after, needle)).toBe(count(base, needle));
      }
    }

    // Excluded native-registry and table-lock statements stay untouched.
    const preview = src(PREVIEW);
    expect(preview).toContain('LOCK TABLE attachment_sets,attachment_links,attachments IN SHARE MODE');
    expect(preview).toContain('nativeRecordCustomerPortalFileAccess(db, event)');
    const denied = src(DENIED);
    expect(denied).toContain('JOIN attachment_sets native_set ON native_set.id=link.set_id');
  });
});

describe('Files read-tail r52 KPI pins', () => {
  test('retirement:ts-statements is 215, strictly below the r51 BEFORE=224 (224->223)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // Two inline heads -> one alias-free owner-local renderer (-1).
    expect(literal).toBe(215);
    expect(total).toBe(216);
    expect(literal).toBeLessThan(224);
    expect(total).toBeLessThan(225);
  });
});
