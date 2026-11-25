import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { notExistsRow, projectFactUnionArms } from '../../../packages/server-projects/src/adapters/read-tail-r37';
import { packageActualSourceProof, projectPackageActualProof, eventFactArm } from '../../../packages/server-projects/src/adapters/package-obligations';
import { projectReadProjection } from '../../../packages/server-projects/src/adapters/project-read-projections';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-r37 emitted arms captured from HEAD before the edit.
// Byte-identity oracles only, never live query sources.
const milestoneArm = `UNION ALL SELECT 'milestone',m.id,m.version,m.state,
 p.id IS NULL OR m.state NOT IN ('completed','cancelled') OR NOT EXISTS
 (SELECT 1 FROM staff_project_milestone_events event WHERE event.milestone_id=m.id AND event.milestone_version=m.version AND event.action=m.state)
 FROM staff_project_milestones m CROSS JOIN scope LEFT JOIN projects p ON p.id=m.project_id WHERE scope.is_base AND m.company_id=scope.company_id`;
const taskArm = `UNION ALL SELECT 'task',t.id,t.version,t.state,p.id IS NULL OR t.state NOT IN ('completed','cancelled')
 OR (t.state='completed' AND NOT EXISTS(SELECT 1 FROM staff_project_task_packages binding JOIN staff_project_milestones m ON m.id=binding.milestone_id
 WHERE binding.task_id=t.id AND binding.company_id=t.company_id AND binding.project_id=t.project_id AND m.project_id=t.project_id AND m.company_id=t.company_id))
 FROM (\${operations.tasks}) t CROSS JOIN scope LEFT JOIN projects p ON p.id=t.project_id WHERE scope.is_base AND t.company_id=scope.company_id AND t.project_id IS NOT NULL`;
const templateArm = `UNION ALL SELECT 'routine-template',t.id,t.version,CASE WHEN t.published=1 THEN 'published' ELSE 'draft' END,p.id IS NULL OR t.published=1
 FROM (\${operations.templates}) t CROSS JOIN scope LEFT JOIN projects p ON p.id=t.project_id WHERE scope.is_base AND t.company_id=scope.company_id AND t.project_id IS NOT NULL`;
const scheduleArm = `UNION ALL SELECT 'routine-schedule',s.id,s.version,s.state,p.id IS NULL OR s.state='active' OR s.company_id IS DISTINCT FROM t.company_id
 FROM (\${operations.schedules}) s JOIN (\${operations.templates}) t ON t.id=s.template_id CROSS JOIN scope LEFT JOIN projects p ON p.id=t.project_id
 WHERE scope.is_base AND s.company_id=scope.company_id AND t.project_id IS NOT NULL`;

const sources = { tasks: '${operations.tasks}', templates: '${operations.templates}', schedules: '${operations.schedules}' };
const workLogWitness = `NOT EXISTS(SELECT 1 FROM staff_project_work_logs l WHERE l.id=a.source_id AND l.project_id=a.project_id AND l.company_id=a.company_id AND l.version=a.source_version AND l.state='approved' AND l.minutes=a.minutes)`;
const costWitness = `NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations c WHERE c.id=a.source_id AND c.project_id=a.project_id AND c.company_id=a.company_id AND c.version=a.source_version AND c.state='approved' AND c.amount_minor=a.amount_minor)`;
const taskBindingWitness = `NOT EXISTS(SELECT 1 FROM staff_project_task_packages b WHERE b.task_id=t.id)`;

describe('Projects read-tail r37 fact union and NOT EXISTS proof dedupe', () => {
  test('projectFactUnionArms reproduces all four former inline UNION arms byte for byte', () => {
    const emitted = projectFactUnionArms(sources);
    expect(emitted).toBe(`${milestoneArm}\n ${taskArm}\n ${templateArm}\n ${scheduleArm}`);
    expect(emitted).toContain("NOT IN ('completed','cancelled')");
    expect(emitted).toContain("CASE WHEN t.published=1 THEN 'published' ELSE 'draft' END");
    expect(emitted).toContain('s.company_id IS DISTINCT FROM t.company_id');
    expect(emitted.match(/scope\.is_base/g)).toHaveLength(4);
    expect(emitted.match(/LEFT JOIN projects p/g)).toHaveLength(4);
    expect(emitted.match(/UNION ALL SELECT /g)).toHaveLength(4);
    expect(emitted.match(/SELECT /g)).toHaveLength(6);
    expect(emitted.split('\n').slice(1).every(line => line.startsWith(' '))).toBe(true);
  });

  test('notExistsRow reproduces all three former inline NOT EXISTS proofs byte for byte', () => {
    expect(notExistsRow('staff_project_work_logs', 'l', "l.id=a.source_id AND l.project_id=a.project_id AND l.company_id=a.company_id AND l.version=a.source_version AND l.state='approved' AND l.minutes=a.minutes")).toBe(workLogWitness);
    expect(notExistsRow('staff_project_cost_allocations', 'c', "c.id=a.source_id AND c.project_id=a.project_id AND c.company_id=a.company_id AND c.version=a.source_version AND c.state='approved' AND c.amount_minor=a.amount_minor")).toBe(costWitness);
    expect(notExistsRow('staff_project_task_packages', 'b', 'b.task_id=t.id')).toBe(taskBindingWitness);
    expect(projectReadProjection.hasTaskBinding).toBe(taskBindingWitness);
    for (const witness of [workLogWitness, costWitness, taskBindingWitness]) {
      expect(witness.startsWith('NOT EXISTS(SELECT 1 FROM ')).toBe(true);
      expect(witness.endsWith(')')).toBe(true);
      expect(witness).not.toContain('?');
      expect(witness.match(/SELECT /g)).toHaveLength(1);
    }
    // Newline positions of the two gap parameters stay exactly as before.
    expect(projectPackageActualProof('work_log', 'l', 'minutes', ' ', '\n ')).toContain("actual.source_kind='work_log' AND actual.source_id=l.id\n AND actual.source_version");
    expect(projectPackageActualProof('cost_allocation', 'c', 'amount_minor', '\n ', ' ')).toContain("actual.source_kind='cost_allocation'\n AND actual.source_id=c.id AND actual.source_version");
    expect(packageActualSourceProof('staff_project_work_logs', 'l', 'minutes')).toBe(workLogWitness);
  });

  test('call sites compose the helpers and former standalone arms are gone', () => {
    const obl = src('package-obligations.ts');
    expect(obl).toContain("{notExistsRow,projectFactUnionArms} from './read-tail-r37'");
    expect(obl).toContain('${projectFactUnionArms(operations)}');
    expect(obl).toContain("notExistsRow('staff_project_package_actuals','actual',");
    expect(obl).toContain('notExistsRow(relation,alias,');
    expect(src('project-read-projections.ts')).toContain("notExistsRow('staff_project_task_packages','b','b.task_id=t.id')");
    expect(obl).not.toContain("UNION ALL SELECT 'milestone',m.id");
    expect(obl).not.toContain("UNION ALL SELECT 'task',t.id");
    expect(obl).not.toContain("UNION ALL SELECT 'routine-template',t.id");
    expect(obl).not.toContain("UNION ALL SELECT 'routine-schedule',s.id");
    expect(obl).not.toContain('NOT EXISTS(SELECT 1 FROM staff_project_package_actuals actual WHERE');
    expect(obl).not.toContain('NOT EXISTS(SELECT 1 FROM ${relation} ${alias} WHERE');
    expect(src('project-read-projections.ts')).not.toContain('NOT EXISTS(SELECT 1 FROM staff_project_task_packages b WHERE');
    // Prior helpers remain and still emit their verbatim arms.
    expect(obl).toContain('export const eventFactArm=');
    expect(eventFactArm({
      kind: 'work-log', alias: 'l', state: 'submitted', table: 'staff_project_work_log_events', op: 'project.actual-time.',
      fkId: 'work_log_id', fkVer: 'work_log_version', actualKind: 'work_log', measure: 'minutes',
      kindGap: ' ', idGap: '\n ', scopeGuard: '', relation: 'staff_project_work_logs', whereScope: 'scope.is_base AND ',
    })).toContain("UNION ALL SELECT 'work-log',l.id,l.version,l.state");
  });

  test('locks, FOR clauses, spaced JOINs and native sources stay untouched', () => {
    const obl = src('package-obligations.ts');
    expect(obl).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones,staff_project_milestone_events,');
    expect(obl).toContain('staff_project_handoffs,command_receipts IN SHARE MODE');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('people-evaluation-facts.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    const scheduled = src('scheduled-tasks.ts');
    expect(scheduled).toContain('FOR UPDATE OF t');
    expect(scheduled).toContain('FOR SHARE');
    expect(src('task-purchases.ts')).toContain(' FOR UPDATE');
    expect(src('executive-evidence-chains.ts')).toContain('JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id');
    expect(src('executive-main-evidence-sources.ts')).toContain('const milestoneEvidenceSelect =');
    expect(readFileSync(resolve(srcRoot, 'adapters/project_actuals.rs'), 'utf8')).toContain('FROM staff_project_cost_allocations');
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
    // r36 left 93. r37 Rank1 folds the four fact UNION arms into one closed
    // helper (-3) and Rank2 folds three NOT EXISTS proofs into notExistsRow (-2).
    expect(total).toBe(71); // r38 member EXISTS closed helper (-4) + evidence super-head (-3): 88->81
    expect(total).toBeLessThan(88);
  });
});
