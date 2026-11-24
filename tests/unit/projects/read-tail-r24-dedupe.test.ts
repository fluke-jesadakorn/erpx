import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { projectsClockCte } from '../../../packages/server-projects/src/adapters/clock-cte';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe literals from HEAD before the r24 edits. These are
// byte-identity oracles only, never live query sources.
const clock = 'floor(extract(epoch FROM clock_timestamp())*1000)::bigint';
const preDedupe = (name: string) => `WITH ${name} AS MATERIALIZED (SELECT ${clock} now_ms)`;

describe('Projects read-tail r24 scheduled/task clock CTE family dedupe', () => {
  test('helper reproduces both former inline clock CTEs byte-identically', () => {
    expect(projectsClockCte('project_alert_clock')).toBe(preDedupe('project_alert_clock'));
    expect(projectsClockCte('project_task_clock')).toBe(preDedupe('project_task_clock'));
    // Only the CTE alias varies; the clock expression and MATERIALIZED mode stay fixed.
    expect(projectsClockCte('project_alert_clock')).toContain(`MATERIALIZED (SELECT ${clock} now_ms)`);
  });

  test('both call sites compose the helper and keep predicates and locks unchanged', () => {
    const alert = src('scheduled-tasks.ts');
    const task = src('task-purchases.ts');
    expect(alert).toContain("projectPurchaseSourceHead('project_alert_clock','project_alert_source','1')");
    expect(task).toContain("projectPurchaseSourceHead('project_task_clock','project_task','1')");
    expect(alert).not.toContain('WITH project_alert_clock AS MATERIALIZED (SELECT');
    expect(task).not.toContain('WITH project_task_clock AS MATERIALIZED (SELECT');
    // The remainder of each source statement is unchanged.
    expect(alert).toContain("projectPurchaseSourceHead('project_alert_clock','project_alert_source','1')}project_alert_source.id=t.project_id");
    expect(task).toContain("projectPurchaseSourceHead('project_task_clock','project_task','1')}project_task.id=t.project_id");
    expect(alert).toContain("purchasedCapabilityPredicate('project_alert_source.company_id',capability,'settle-existing','project_alert_clock.now_ms')");
    expect(task).toContain("purchasedCapabilityPredicate('project_task.company_id','projects.tasks','history','project_task_clock.now_ms')");
    // Lock order, transaction boundary and deferred effects are not edited.
    expect(locksUntouched()).toBe(true);
  });

  test('r2-r23 helpers, source and locks stay untouched', () => {
    expect(src('clock-cte.ts')).toContain('export const projectsClockCte=');
    expect(src('package-obligations.ts')).toContain('export const projectAssignmentScopeSql=');
    expect(src('package-obligations.ts')).toContain('const tableRowsSql=`SELECT * FROM `;');
    const defs = src('project-read-projections.ts');
    expect(defs).toContain('export const projectMemberScopeGuard =');
    expect(defs).toContain('export const projectMemberScopeSql =');
    expect(src('final-root-owner-projection-sources.ts')).toContain('const quotedProjection =');
    expect(src('collaboration-home-sources.ts')).toContain('const projectsIdTitleOptions =');
    expect(src('executive-metric-projections.ts')).toContain('const groupedMetricSelect =');
    expect(src('read-tail-r23.ts')).toContain('projectAssignmentAuthorityReadSql');
  });

  test('retirement:ts-statements is strictly lower than BEFORE=124', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(124);
    expect(total).toBeLessThanOrEqual(123);
  });
});

function locksUntouched(): boolean {
  const alert = src('scheduled-tasks.ts');
  const task = src('task-purchases.ts');
  return alert.includes('companyPurchaseReadLocks(database,target.companyId)')
    && alert.includes('"SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR SHARE"')
    && alert.includes('"SELECT id FROM assignments WHERE id=? FOR SHARE"')
    && alert.includes('FOR UPDATE OF t')
    && task.includes('taskSource(database,actor,command.id,true)')
    && task.includes('workContextGuards(database,actor,key)')
    && task.includes('iamMutationGuards(database,actor,key)')
    && task.includes('deferProjectOwnerCheck(database,actor.companyId,');
}
