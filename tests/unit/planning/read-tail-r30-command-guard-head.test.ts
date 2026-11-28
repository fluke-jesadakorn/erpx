import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningCommandExistsGuard } from '../../../modules/planning/server/src/adapters/planning-command-guard-sql';

const srcRoot = resolve(__dirname, '../../../modules/planning/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

// Verbatim pre-r30 EXISTS guards. Byte-identity oracles only, never live sources.
const COMPARISON = 'EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND branch_id=? AND specification_id=? AND version=?)';
const UNRELEASED = "EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND version=? AND status='calculated')";
const INDEPENDENT = "EXISTS(SELECT 1 FROM planning_recommendations WHERE id=? AND version=? AND status='proposed' AND created_by<>?)";
const RESOURCE = "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND status='active')";

describe('Planning r30 command-guard EXISTS head retirement', () => {
  test('the generalized fact builder reproduces every retired EXISTS guard byte-for-byte', () => {
    const guards = [
      planningCommandExistsGuard('planning_scenarios', 'id=? AND company_id=? AND branch_id=? AND specification_id=? AND version=?'),
      planningCommandExistsGuard('planning_scenarios', "id=? AND company_id=? AND version=? AND status='calculated'"),
      planningCommandExistsGuard('planning_recommendations', "id=? AND version=? AND status='proposed' AND created_by<>?"),
      planningCommandExistsGuard('production_resources', "id=? AND company_id=? AND status='active'"),
    ];
    expect(guards).toEqual([COMPARISON, UNRELEASED, INDEPENDENT, RESOURCE]);
    expect(guards.map(sha)).toEqual([
      '3c5bbfc5e330952166087198a82d651f7b389dade5bd91a6b867726d512e3845',
      'fc0b624b23a008d245082eb00794959846595dd1bdd03eb9b4a419f41aa443f3',
      '659f4e8917eb7f3e8f5417dfc8d2a6b248ea70bba024ae533f8407c9f7255f66',
      '9e790ead82c0f7536163dc334c272ae59d5d0eccb995c074df5e6c93efce6059',
    ]);
    for (const sql of guards) {
      expect(selectCount(sql)).toBe(1);
      expect(sql.startsWith('EXISTS(')).toBe(true);
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('the replaced EXISTS literal is deleted; the wrapper keeps its export and call sites', () => {
    const wrapper = src('planning-command-guard-sql.ts');
    expect(selectCount(wrapper)).toBe(0);
    expect(wrapper).toContain("EXISTS(${planningPackageFactRows('1', table, predicate)})");
    expect(wrapper).toContain('export const planningCommandExistsGuard =');
    for (const name of ['planning-guard-sql.ts', 'planning-attachment-target-sql.ts', 'production-command-sources-read.ts'])
      expect(src(name)).toContain('planningCommandExistsGuard(');
  });

  test('retirement:ts-statements drops 10 -> 9', () => {
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
    expect(total).toBeLessThan(10);
  });
});
