import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningAllocatedMinutes, productionResourcesExists } from '../../../modules/planning/server/src/adapters/planning-guard-sql';

const srcRoot = resolve(__dirname, '../../../modules/planning/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

// Verbatim pre-r31 guards. Byte-identity oracles only, never live query sources.
const SUM = 'COALESCE((SELECT sum(minutes) FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)';
const RESOURCE = "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND status='active')";

describe('Planning r31 allocated-minutes sum head retirement', () => {
  test('the generalized fact builder reproduces the retired sum guard byte-for-byte', () => {
    const resource = productionResourcesExists("status='active'");
    expect(planningAllocatedMinutes).toBe(SUM);
    expect(resource).toBe(RESOURCE);
    expect([SUM, resource].map(sha)).toEqual([
      'b761f6facdf6bf2f5622aa9662c87e388f0170beb45126e16dca8196fbf79b5f',
      '9e790ead82c0f7536163dc334c272ae59d5d0eccb995c074df5e6c93efce6059',
    ]);
    for (const sql of [SUM, resource]) {
      expect(selectCount(sql)).toBe(1);
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('the replaced SELECT literal is deleted; the wrapper keeps both exports', () => {
    const wrapper = src('planning-guard-sql.ts');
    expect(selectCount(wrapper)).toBe(0);
    expect(wrapper).toContain("planningPackageFactRows('sum(minutes)', 'planning_resource_allocations'");
    expect(wrapper).toContain('export const planningAllocatedMinutes =');
    expect(wrapper).toContain('export const productionResourcesExists =');
    for (const name of ['resources-read.ts', 'production-command-sources-read.ts'])
      expect(src(name)).toContain('planningAllocatedMinutes');
  });

  test('retirement:ts-statements drops 9 -> 8', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBe(5);
    expect(total).toBeLessThan(9);
  });
});
