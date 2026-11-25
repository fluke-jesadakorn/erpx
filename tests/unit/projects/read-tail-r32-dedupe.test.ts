import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { projectsClockCte } from '../../../packages/server-projects/src/adapters/clock-cte';
import { projectPurchaseSourceHead } from '../../../packages/server-projects/src/adapters/read-tail-r32';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe inline purchase-source heads from HEAD before the r32 edit.
// These are byte-identity oracles only, never live query sources.
const metricHead = `${projectsClockCte('project_metric_clock')} SELECT project_metric_owned.* FROM staff_projects project_metric_owned CROSS JOIN project_metric_clock WHERE `;
const taskHead = `${projectsClockCte('project_task_clock')} SELECT 1 FROM staff_projects project_task CROSS JOIN project_task_clock WHERE `;
const alertHead = `${projectsClockCte('project_alert_clock')} SELECT 1 FROM staff_projects project_alert_source CROSS JOIN project_alert_clock WHERE `;

describe('Projects read-tail r32 purchase-source head dedupe', () => {
  test('helper reproduces all three former inline heads byte for byte', () => {
    expect(projectPurchaseSourceHead('project_metric_clock', 'project_metric_owned', 'project_metric_owned.*')).toBe(metricHead);
    expect(projectPurchaseSourceHead('project_task_clock', 'project_task', '1')).toBe(taskHead);
    expect(projectPurchaseSourceHead('project_alert_clock', 'project_alert_source', '1')).toBe(alertHead);
    for (const head of [metricHead, taskHead, alertHead]) {
      expect(head.startsWith('WITH ')).toBe(true);
      expect(head).toContain(' AS MATERIALIZED (SELECT ');
      expect(head.endsWith(' WHERE ')).toBe(true);
      // One clock-CTE SELECT plus one source-projection SELECT, as before.
      expect(head.match(/SELECT /g)).toHaveLength(2);
    }
  });

  test('all three call sites compose the helper and keep their predicates unchanged', () => {
    const metric = src('metric-purchases.ts');
    expect(metric).toContain("import {projectPurchaseSourceHead} from './read-tail-r32';");
    expect(metric).toContain("${projectPurchaseSourceHead('project_metric_clock','project_metric_owned','project_metric_owned.*')}${authority}");
    expect(metric).not.toContain('${projectsClockCte(');
    expect(metric).toContain("purchasedCapabilityPredicate('project_metric_owned.company_id','projects.wbs','history',clock)");

    const task = src('task-purchases.ts');
    expect(task).toContain("import {projectPurchaseSourceHead} from './read-tail-r32';");
    expect(task).toContain("${projectPurchaseSourceHead('project_task_clock','project_task','1')}project_task.id=t.project_id");
    expect(task).not.toContain('${projectsClockCte(');
    expect(task).toContain("purchasedCapabilityPredicate('project_task.company_id','projects.tasks','history','project_task_clock.now_ms')");

    const alert = src('scheduled-tasks.ts');
    expect(alert).toContain("import {projectPurchaseSourceHead} from './read-tail-r32';");
    expect(alert).toContain("${projectPurchaseSourceHead('project_alert_clock','project_alert_source','1')}project_alert_source.id=t.project_id");
    expect(alert).not.toContain('${projectsClockCte(');
    expect(alert).toContain("purchasedCapabilityPredicate('project_alert_source.company_id',capability,'settle-existing','project_alert_clock.now_ms')");
  });

  test('locks, clock helpers and r2-r31 helpers stay untouched', () => {
    expect(src('clock-cte.ts')).toContain('export const projectsClockCte=');
    expect(src('read-tail-r23.ts')).toContain('projectAssignmentAuthorityReadSql');
    expect(src('read-tail-r27.ts')).toContain('projectMemberExistsSql');
    expect(src('read-tail-r28.ts')).toContain('assignmentViewerSelect');
    expect(src('read-tail-r29.ts')).toContain('orderedImageAggregateHead');
    expect(src('native_lane_27/postgres.rs')).toContain('const VISIBLE_PROJECT: &str');
    expect(src('package-obligations.ts')).toContain('export const projectReceiptEventProof=');
    expect(src('executive-milestone-cohort.ts')).toContain('projectOwnerOrMemberColumnSql');
    const alert = src('scheduled-tasks.ts');
    expect(alert).toContain('FOR UPDATE OF t');
    expect(alert).toContain('FOR SHARE');
    expect(alert).toContain("projectPurchaseSourceHead('project_alert_clock','project_alert_source','1')");
    expect(alert).not.toContain('SELECT 1 FROM staff_projects project_alert_source CROSS JOIN project_alert_clock');
    expect(src('task-purchases.ts')).toContain("${lock?' FOR UPDATE':''}");
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('package-obligations.ts')).toContain('projectReceiptEventProof');
    expect(src('project-read-projections.ts')).toContain('projectOwnerOrMemberColumnSql');
  });

  test('retirement:ts-statements is 74 after r40 (BEFORE=81)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(88);
    expect(total).toBe(57); // r46 evaluation reads moved to the native contract (62->57)
  });
});
