import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { resourceCalendarByKey } from '../../../packages/server-planning/src/adapters/planning-calendar-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe literals from HEAD for the two production_resource_calendar
// point lookups keyed by resource_id/work_date. Byte-identity oracles only,
// never live query sources.
const CAPACITY_CALENDAR = 'SELECT available_minutes,version FROM production_resource_calendar WHERE resource_id=? AND work_date=?';
const VERSION_GUARD = 'COALESCE((SELECT version FROM production_resource_calendar WHERE resource_id=? AND work_date=?),0)=?';

const generated = [
  resourceCalendarByKey('available_minutes,version'),
  `COALESCE((${resourceCalendarByKey('version')}),0)=?`,
];

describe('Planning read-tail r10 parameterized resource-calendar point-lookup family', () => {
  test('calendar builder reproduces both originals byte-identically', () => {
    expect(generated).toEqual([CAPACITY_CALENDAR, VERSION_GUARD]);
  });

  test('both former sites compose the builder and keep their own prepare/bind', () => {
    const capacity = src('capacity.ts');
    expect(capacity).not.toContain(CAPACITY_CALENDAR);
    expect(capacity).toContain("resourceCalendarByKey('available_minutes,version')");
    expect(capacity).toContain('.bind(resourceId, date).first<{ available_minutes: number; version: number }>()');
    expect(capacity).toContain('planning-calendar-source-sql');

    const resources = src('resources-read.ts');
    expect(resources).not.toContain(VERSION_GUARD);
    expect(resources).toContain("resourceCalendarByKey('version')");
    expect(resources).toContain('calendarVersionGuard');
    expect(resources).toContain('planning-calendar-source-sql');
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-calendar-source-sql.ts');
    expect(selectCount(shared)).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const sql of generated) {
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('r4-r9 helpers, native read consumers and all lock statements stay untouched', () => {
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-command-guard-sql.ts')).toContain('planningCommandExistsGuard');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetRow');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetVersionExists');
    expect(src('planning-scoped-list-sql.ts')).toContain('planningScopedBranchList');
    expect(src('planning-scoped-row-sql.ts')).toContain('planningScopedBranchRow');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('capacity.ts')).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    expect(src('resources-read.ts')).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);

    const nativeRead = readFileSync(resolve(srcRoot, 'adapters', 'planning_read.rs'), 'utf8');
    expect(nativeRead).toContain('pub async fn workspace');
    expect(nativeRead).toContain('pub fn parse_shortages');
  });

  test('retirement:ts-statements stays strictly below the r10 BEFORE=32 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(32);
    expect(total).toBeLessThanOrEqual(31);
  });
});
