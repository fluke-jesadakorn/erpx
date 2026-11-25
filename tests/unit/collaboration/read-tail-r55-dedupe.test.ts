import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  readablePostHead,
  socialPostByIdGuard,
  socialPostByIdPredicate,
  socialPostScopedRead,
} from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/read-tail-sql.ts'), 'utf8');
const line = (marker: string) => shared.split('\n').find(l => l.startsWith(marker))!;

// Exact pre-r55 renderings at the live HEAD. The three helpers shared one
// Social-post scoped head; only the projected columns, alias and tail varied.
const PREDICATE = "operationalHandoverReceiptPredicate('p')";
const GUARD = 'author_id=?';
const PROJECTION = "p.state='published'";

describe('Collaboration read-tail r55 Social-post scoped head family', () => {
  test('the single renderer reproduces all three retired heads byte for byte', () => {
    expect(socialPostByIdPredicate(PREDICATE)).toBe(`SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${PREDICATE}`);
    expect(socialPostByIdGuard(GUARD)).toBe(`SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND ${GUARD}`);
    expect(readablePostHead('*', PROJECTION)).toBe(`SELECT p.* FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${PROJECTION}`);
    for (const sql of [
      socialPostByIdPredicate(PREDICATE),
      socialPostByIdGuard(GUARD),
      readablePostHead('*', PROJECTION),
    ]) {
      expect((sql.match(/SELECT /g) ?? []).length).toBe(1);
      expect((sql.match(/\?/g) ?? []).length).toBeGreaterThanOrEqual(2);
      expect(sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i);
    }
  });

  test('one owner renderer and three thin wrappers, no inline SELECT left', () => {
    expect(shared.match(/export const socialPostScopedRead=/g) ?? []).toHaveLength(1);
    expect((line('export const socialPostScopedRead=').match(/SELECT /g) ?? []).length).toBe(1);
    const renderer = line('export const socialPostScopedRead=');
    expect(renderer).toContain('FROM social_posts${alias?` ${alias}`:');
    expect(renderer).toContain('WHERE ${alias?`${alias}.`:');
    expect(renderer).toContain('id=? AND ${alias?`${alias}.`:');
    expect(renderer).toContain('company_id=? AND ${tail}`;');
    for (const marker of [
      'export const socialPostByIdPredicate=',
      'export const socialPostByIdGuard=',
      'export const readablePostHead=',
    ]) {
      expect(line(marker)).toContain('socialPostScopedRead(');
      expect(line(marker)).not.toContain('SELECT ');
    }
  });

  test('call sites keep their names, tails and bind order', () => {
    const files = ['work-social-draft-policy.ts', 'work-social-purchases.ts', 'work-social-read-policy.ts', 'social-home.ts', 'social.ts'];
    let calls = 0;
    for (const file of files) {
      const body = readFileSync(resolve(srcRoot, 'adapters', file), 'utf8');
      calls += (body.match(/socialPostByIdPredicate\(|socialPostByIdGuard\(|readablePostHead\(/g) ?? []).length;
    }
    expect(calls).toBeGreaterThan(0);
    expect(shared).not.toContain('`SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND ${tail}`');
  });
});

describe('Collaboration read-tail r55 retirement check', () => {
  test('retirement:ts-statements drops 302 -> 300', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(247);
    expect(total).toBeLessThan(302);
  });
});
