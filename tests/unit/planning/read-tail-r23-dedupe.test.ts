import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { resourceCalendarByKey } from '../../../modules/planning/server/src/adapters/planning-calendar-source-sql';

const srcRoot = resolve(__dirname, '../../../modules/planning/server/src');
/** Pre-r28 byte oracle for the retired builder; no longer a live query source. */
const planningSelectColumnsFrom = (columns: string, rest: string) =>
  `SELECT ${columns} FROM ${rest}`;
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-r23 renderings from HEAD for the two production_resource_calendar
// point lookups keyed by resource_id/work_date. Byte-identity oracles only,
// never live query sources: the thin wrapper must emit these exactly after
// folding into the generalized single-line projection/FROM builder.
const CAPACITY_CALENDAR = 'SELECT available_minutes,version FROM production_resource_calendar WHERE resource_id=? AND work_date=?';
const VERSION_GUARD = 'COALESCE((SELECT version FROM production_resource_calendar WHERE resource_id=? AND work_date=?),0)=?';
const CALENDAR_TAIL = 'production_resource_calendar WHERE resource_id=? AND work_date=?';

const generated = [
  resourceCalendarByKey('available_minutes,version'),
  `COALESCE((${resourceCalendarByKey('version')}),0)=?`,
];

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Planning read-tail r23 resource-calendar point-lookup fold', () => {
  test('thin wrapper reproduces both pre-r23 statements byte-identically', () => {
    expect(generated).toEqual([CAPACITY_CALENDAR, VERSION_GUARD]);
    expect(generated[0]).toBe(planningSelectColumnsFrom('available_minutes,version', CALENDAR_TAIL));
    expect(generated[1]).toBe(`COALESCE((${planningSelectColumnsFrom('version', CALENDAR_TAIL)}),0)=?`);
    generated.forEach(readOnly);
  });

  test('wrapper delegates and the generalized builder owns the single read literal', () => {
    const wrapper = src('planning-calendar-source-sql.ts');
    expect(selectCount(wrapper)).toBe(0);
    expect(wrapper).toContain("planningPackageFactRows(columns, 'production_resource_calendar', 'resource_id=? AND work_date=?')");
    expect(wrapper).not.toContain('SELECT ');
    expect(wrapper.match(/export const resourceCalendarByKey =/g) ?? []).toHaveLength(1);
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-columns-sql.ts'))).toBe(false);
  });

  test('both callers keep composing the wrapper with unchanged binds', () => {
    const capacity = src('capacity.ts');
    expect(capacity).toContain("resourceCalendarByKey('available_minutes,version')");
    expect(capacity).toContain('.bind(resourceId, date).first<{ available_minutes: number; version: number }>()');
    expect(capacity).toContain('planning-calendar-source-sql');
    expect(capacity).not.toContain(CAPACITY_CALENDAR);
    expect(capacity).not.toContain('SELECT ');

    const resources = src('resources-read.ts');
    expect(resources).toContain("resourceCalendarByKey('version')");
    expect(resources).toContain('calendarVersionGuard');
    expect(resources).toContain('planning-calendar-source-sql');
    expect(resources).not.toContain(VERSION_GUARD);
    expect(resources).not.toContain('SELECT ');
  });

  test('r1-r22 helpers, the single lock statement and native readers stay untouched', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-columns-sql.ts'))).toBe(false);
    expect(readdirSync(resolve(srcRoot, 'adapters'))).not.toContain('planning-select-from-sql.ts');
    expect(src('planning-package-fact-sql.ts')).toContain('export const planningPackageFactRows =');
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-command-guard-sql.ts')).toContain('export const planningCommandExistsGuard =');
    expect(src('planning-attachment-target-sql.ts')).toContain('export const planningAttachmentTargetRow =');
    expect(src('planning-attachment-target-sql.ts')).toContain('export const planningAttachmentTargetVersionExists =');
    expect(src('planning-scoped-list-sql.ts')).toContain('export const planningScopedBranchList =');
    expect(existsSync(resolve(srcRoot, 'adapters/planning-scoped-row-sql.ts'))).toBe(false);
    expect(src('planning-image-source-sql.ts')).toContain('export const planningSourceRows =');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    let locks = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) locks += (readFileSync(path, 'utf8').match(/LOCK TABLE/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(locks).toBe(1);
    for (const name of ['resources-read.ts', 'production-command-sources-read.ts', 'capacity.ts']) {
      expect(src(name)).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    }
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) {
        const text = readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8');
        expect(text).not.toContain('resourceCalendarByKey');
        expect(text).not.toContain('planningSelectColumnsFrom');
      }
    }
  });

  test('retirement:ts-statements strictly lowers the r22 BEFORE=16 baseline to 15', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r24 folded the scoped single-row family into the single-line projection/FROM builder (15 -> 14).
    // r25 folded the scoped branch-list family into the single-line projection/FROM builder (14 -> 13).
    expect(total).toBe(5);
    expect(total).toBeLessThan(16);
  });
});
