import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { packageClockSql } from '@erpx/server-commercial/public/purchased-capability-sql';
import { operationsTableExists, operationsOwnerVersionExists } from '../../../packages/server-operations/src/adapters/identity-policy-version-guard';
import { OPERATIONS_CLOCK_CTE_SQL, OPERATIONS_VIEWER_CLOCK_SQL, operationsViewerClockHead } from '../../../packages/server-operations/src/adapters/workforce-actor-scope-source';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');

describe('operations read-tail r49 EXISTS/CTE-head retirement', () => {
  it('renders the three family-1 EXISTS probes byte-identically through operationsTableExists', () => {
    // Exact retired literals captured before this change; only the source and
    // predicate are handed to the existing zero-new-definition helper.
    const routinePredicate = "routine_owner.id=? AND routine_owner.company_id=t.company_id AND routine_owner.scope_kind='holding' AND iam_role_allows(routine_owner.role,routine_owner.company_id,'${permission}')";
    expect(operationsTableExists('assignments routine_owner', routinePredicate))
      .toBe("EXISTS(SELECT 1 FROM assignments routine_owner WHERE routine_owner.id=? AND routine_owner.company_id=t.company_id AND routine_owner.scope_kind='holding' AND iam_role_allows(routine_owner.role,routine_owner.company_id,'${permission}'))");
    expect(operationsTableExists('handbook_editors editor', 'editor.company_id=a.company_id AND editor.user_id=?'))
      .toBe("EXISTS(SELECT 1 FROM handbook_editors editor WHERE editor.company_id=a.company_id AND editor.user_id=?)");
    const user = 'work_record_viewer.user_id';
    expect(operationsTableExists('staff_project_members m', `m.project_id=parent_project.id AND m.user_id=${user}`))
      .toBe(`EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=parent_project.id AND m.user_id=${user})`);
    // Still the single canonical owner-version renderer; no new definition.
    expect(operationsTableExists('handbook_editors editor', 'x'))
      .toBe(operationsOwnerVersionExists('handbook_editors editor', '', 'x'));
    expect(operationsTableExists('t', 'p').match(/SELECT /g)).toHaveLength(1);
  });

  it('operationsViewerClockHead preserves the newline-space separator and viewer alias', () => {
    expect(operationsViewerClockHead('work_record_viewer'))
      .toBe(`EXISTS(WITH work_record_viewer AS MATERIALIZED${OPERATIONS_VIEWER_CLOCK_SQL}\n SELECT 1 FROM `);
    expect(operationsViewerClockHead('work_task_file_actor'))
      .toBe(`EXISTS(WITH work_task_file_actor AS MATERIALIZED${OPERATIONS_VIEWER_CLOCK_SQL}\n SELECT 1 FROM `);
    // The FROM table and predicates remain local to each caller.
    const head = operationsViewerClockHead('work_record_viewer');
    expect(head + 'staff_tasks t CROSS JOIN work_record_viewer')
      .toBe(`EXISTS(WITH work_record_viewer AS MATERIALIZED${OPERATIONS_VIEWER_CLOCK_SQL}\n SELECT 1 FROM staff_tasks t CROSS JOIN work_record_viewer`);
    expect(head.endsWith('\n SELECT 1 FROM ')).toBe(true);
    // The head adds one static SELECT token in source; the rendered SQL also
    // carries the embedded viewer-clock CTE's own SELECT.
    expect(operationsViewerClockHead('v').match(/SELECT /g)).toHaveLength(2);
  });

  it('clock CTE constants stay byte-identical behind the closed bound flag', () => {
    expect(OPERATIONS_CLOCK_CTE_SQL).toBe(`(SELECT ${packageClockSql} now_ms)`);
    expect(OPERATIONS_VIEWER_CLOCK_SQL).toBe(`(SELECT ?::text user_id,?::text assignment_id,${packageClockSql} now_ms)`);
  });

  it('retires every inline copy and keeps one definition per family', async () => {
    const [routine, purchases, policy, evaluation, attachment, scope, guard, alert] = await Promise.all([
      src('routine-source-read.ts'), src('record-purchases.ts'), src('project-task-record-policy.ts'),
      src('evaluation-source-policy.ts'), src('task-attachment-purchases.ts'),
      src('workforce-actor-scope-source.ts'), src('identity-policy-version-guard.ts'),
      src('work-task-alert-purchases.ts'),
    ]);
    // Family 1: three converted sites, no inline copies left.
    expect(routine.match(/operationsTableExists\(/g)).toHaveLength(1);
    expect(routine).not.toContain('EXISTS(SELECT 1 FROM assignments routine_owner');
    expect(purchases.match(/operationsTableExists\(/g)).toHaveLength(3);
    expect(purchases).not.toContain('EXISTS(SELECT 1 FROM handbook_editors');
    expect(policy.match(/operationsTableExists\(/g)).toHaveLength(1);
    expect(policy).not.toContain('EXISTS(SELECT 1 FROM staff_project_members');
    expect(guard.match(/export const operationsTableExists/g)).toHaveLength(1);
    // Family 2: both head pairs converted; r54 routed the single-line
    // projection site through the same head with a space separator.
    expect(evaluation.match(/operationsViewerClockHead\(/g)).toHaveLength(1);
    expect(evaluation).not.toContain('EXISTS(WITH ${viewer} AS MATERIALIZED');
    expect(evaluation).not.toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(attachment.match(/operationsViewerClockHead\(/g)).toHaveLength(1);
    expect(attachment).not.toContain('EXISTS(WITH ${viewer} AS MATERIALIZED');
    expect(attachment).not.toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(purchases).toContain('${operationsViewerClockHead(viewer,\' \')}${viewer} CROSS JOIN LATERAL');
    expect(purchases).not.toContain('EXISTS(WITH ${viewer} AS MATERIALIZED${OPERATIONS_VIEWER_CLOCK_SQL}');
    expect(scope.match(/export const operationsViewerClockHead/g)).toHaveLength(1);
    expect(scope.match(/operationsClockCte\(/g)).toHaveLength(2); // false + true call sites
    // r50 retired the spare iam_roles probe; LOCK and native reads stay untouched.
    expect(alert.match(/operationsTableExists\(/g)).toHaveLength(1);
    expect(alert).not.toContain('EXISTS(SELECT 1 FROM iam_roles role');
    expect(alert).toContain('LOCK TABLE staff_tasks');
    expect(alert).toContain('OPERATIONS_RD_WORK_TASK_ALERT_IMAGE');
    expect(purchases).toContain('nativeOperationsReadFirst');
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 89 to 75', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(67);
    expect(total).toBeLessThan(89);
  });
});
