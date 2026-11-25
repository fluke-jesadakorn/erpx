import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { resourceCalendarByKey } from '../../../packages/server-planning/src/adapters/planning-calendar-source-sql';
import { planningScopedBranchList } from '../../../packages/server-planning/src/adapters/planning-scoped-list-sql';
import { planningPackageFactRows } from '../../../packages/server-planning/src/adapters/planning-package-fact-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const norm = (text: string) => text.replace(/\s+/g, ' ').trim();

// Verbatim pre-r28 renderings of the retired head, evaluated with fixed
// server-owned arguments. They are byte-identity oracles, never live sources.
const SPEC = '<SCENARIO_SPEC>';
const CALENDAR = 'SELECT version FROM production_resource_calendar WHERE resource_id=? AND work_date=?';
const SCOPED = `SELECT p.*,s.name specification_name FROM planning_scenarios p ${SPEC} WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) ORDER BY p.updated_at DESC`;
const OPERATIONS = norm('SELECT o.*,r.version resource_version,r.status,r.daily_minutes,r.work_weekdays_json,r.branch_id FROM manufacturing_specification_operations o JOIN production_resources r ON r.id=o.resource_id\n        WHERE o.specification_id=? AND r.company_id=? ORDER BY o.ordinal');

describe('Planning r28 calendar/guard SELECT head retirement', () => {
  test('the surviving generalized builder reproduces every retired statement', () => {
    const calendar = resourceCalendarByKey('version');
    const scoped = planningScopedBranchList('p.*,s.name specification_name', `planning_scenarios p ${SPEC}`, 'p.company_id=?', 'p.branch_id=?', 'p.updated_at DESC');
    const operations = planningPackageFactRows('o.*,r.version resource_version,r.status,r.daily_minutes,r.work_weekdays_json,r.branch_id', 'manufacturing_specification_operations o JOIN production_resources r ON r.id=o.resource_id', 'o.specification_id=? AND r.company_id=? ORDER BY o.ordinal');
    expect(calendar).toBe(CALENDAR);
    expect(scoped).toBe(SCOPED);
    // The capacity read folds the former newline/indent before WHERE into the
    // generalized builder's single space; only whitespace changes.
    expect(norm(operations)).toBe(OPERATIONS);
    expect([calendar, scoped, norm(operations)].map(sha)).toEqual([
      'a4a18aec1fbf256f0c88041e6dfd187c1a2b4c0286dce04e45e875249fd11678',
      'aadd818cfa34f8eddcf228188fafdefd22cf666294aceced7a240b37f4fb4350',
      '8174303913dc27c2c4a59a12a2b93d4406fd7a6171201cbbea7fc645a56f5e30',
    ]);
    for (const sql of [calendar, scoped, operations]) {
      expect(selectCount(sql)).toBe(1);
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('the retired head and its import are deleted; thin wrappers stay SQL-free', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-columns-sql.ts'))).toBe(false);
    expect(readdirSync(resolve(srcRoot, 'adapters'))).not.toContain('planning-select-columns-sql.ts');
    for (const name of ['planning-calendar-source-sql.ts', 'planning-scoped-list-sql.ts']) {
      const wrapper = src(name);
      expect(selectCount(wrapper)).toBe(0);
      expect(wrapper).toContain('planningPackageFactRows');
      expect(wrapper).not.toContain('planning-select-columns-sql');
    }
    expect(src('capacity.ts')).toContain('planningPackageFactRows(');
    expect(src('capacity.ts')).not.toContain('planningSelectColumnsFrom');
  });

  test('transaction-bound guards keep their exact predicates on the caller connection', () => {
    const resources = src('resources-read.ts');
    expect(resources).toContain("calendarVersionGuard = `COALESCE((${resourceCalendarByKey('version')}),0)=?`");
    expect(resources).toContain('export function planningContextGuards(');
    expect(src('planning-guard-sql.ts')).toContain('planningCommandExistsGuard(');
    expect(src('planning-command-guard-sql.ts')).toContain('EXISTS(SELECT 1 FROM ${table} WHERE ${predicate})');
  });

  test('retirement:ts-statements drops 12 -> 11', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBe(10);
    expect(total).toBeLessThan(12);
  });
});
