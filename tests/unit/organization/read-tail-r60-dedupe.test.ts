import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { levelHistorySql } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r60';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Head written out independently of the r60 renderer body.
const HEAD = 'SELECT h.*,l.name level_name';
const oracle = (tail: string) => `${HEAD}${tail}`;

const APP_TAIL = ',old.name previous_level_name FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id LEFT JOIN staff_levels old ON old.id=h.previous_level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC';
const VIEW_TAIL = ' FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC';

type Case = { name: string; tail: string; rendered: string; sha: string; binds: number };

// Byte oracles copied verbatim from the two pre-r60 (BEFORE=208) inline call
// sites. Only the head moves to the renderer; each tail, ? order and bind
// arity is preserved exactly.
const CASES: Case[] = [
  {
    name: 'person-level-history / previous level',
    tail: APP_TAIL,
    rendered: "SELECT h.*,l.name level_name,old.name previous_level_name FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id LEFT JOIN staff_levels old ON old.id=h.previous_level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC",
    sha: 'b1d124381bceafd9a2194d307b6461aa1005121e3d26ddca040d02bab8596fcd',
    binds: 1,
  },
  {
    name: 'member-profile-level-history / current level',
    tail: VIEW_TAIL,
    rendered: "SELECT h.*,l.name level_name FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC",
    sha: 'eb6d226782b8b7034da4bf407a80df3394e6d9b1425535297055a2505a7c0aae',
    binds: 1,
  },
];

describe('Organization read-tail r60 dedupe', () => {
  test('renderer reproduces both pre-r60 SQL strings byte for byte', () => {
    expect(CASES).toHaveLength(2);
    for (const c of CASES) {
      const got = levelHistorySql(c.tail);
      expect(got, c.name).toBe(c.rendered);
      expect(got, c.name).toBe(oracle(c.tail));
      expect(sha(got), c.name).toBe(c.sha);
      expect(count(got, 'SELECT ')).toBe(1);
      expect((got.match(/\?/g) ?? []).length, c.name).toBe(c.binds);
      expect(got.startsWith(HEAD)).toBe(true);
      expect(got).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE|INSERT INTO|DELETE FROM/i);
    }
    // One head, distinct rendered tails; the bind arity is unchanged.
    expect(CASES[0].tail).not.toBe(CASES[1].tail);
    expect(CASES[0].sha).not.toBe(CASES[1].sha);
    expect(CASES[0].binds).toBe(CASES[1].binds);
  });

  test('both call sites delegate with unchanged tails and binds', () => {
    const app = read('people-app-entrypoint-facts.ts');
    const view = read('member-profile-view.ts');
    expect(app).toContain("import { levelHistorySql } from './organization-read-tail-sources-r60';");
    expect(view).toContain('import { levelHistorySql } from "./organization-read-tail-sources-r60";');
    expect(count(app, 'levelHistorySql(')).toBe(1);
    expect(count(view, 'levelHistorySql(')).toBe(1);
    for (const c of CASES) {
      expect(app).not.toContain(c.rendered);
      expect(view).not.toContain(c.rendered);
    }
    expect(app).toContain(`levelHistorySql("${APP_TAIL}")`);
    expect(view).toContain(`levelHistorySql("${VIEW_TAIL}")`);
    // Bind arguments stay at the call sites, in original order and arity.
    expect(app).toContain('.bind(engagementId)');
    expect(view).toContain('.bind(engagementId)');
  });

  test('helper owns one read-only head; locks, spaced joins and r57/r59 helpers stay untouched', () => {
    const helper = read('organization-read-tail-sources-r60.ts');
    expect(count(helper, 'export const levelHistorySql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper).toContain('`SELECT h.*,l.name level_name${tail}`');
    expect(helper).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE|pg_advisory/i);

    expect(read('organization-read-tail-sources-r57.ts')).toContain('export const authoritySeatExistsSql =');
    expect(read('organization-read-tail-sources-r59.ts')).toContain('export const executiveMetricCountSql =');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('people-lifecycle-facts.ts')).toContain('FOR UPDATE');
    expect(read('organization-drafts.ts')).toContain('FOR UPDATE');
    expect(read('project-worker-startup-sources.ts')).toContain(
      'LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE',
    );
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements pins 208 -> 207', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r60 folds two level-history head literals into one renderer
    // r61 folds two position-package projection heads (-2 literals, +1 renderer): 207 - 1 = 206.
    // (-2 literals, +1 declaration): 208 - 2 + 1 = 207.
    expect(CASES).toHaveLength(2);
    expect(total).toBe(205);
    expect(total).toBe(208 - 3);
    expect(total).toBeLessThan(208);
  });
});
