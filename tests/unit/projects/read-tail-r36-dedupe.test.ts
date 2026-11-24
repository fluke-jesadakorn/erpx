import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import {
  packageActualSourceProof,
  eventFactArm,
  projectReceiptEventProof,
  projectPackageActualProof,
} from '../../../packages/server-projects/src/adapters/package-obligations';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-r36 emitted fragments captured from HEAD before the edit.
// Byte-identity oracles only, never live query sources.
const workLogWitness = `NOT EXISTS(SELECT 1 FROM staff_project_work_logs l WHERE l.id=a.source_id AND l.project_id=a.project_id AND l.company_id=a.company_id AND l.version=a.source_version AND l.state='approved' AND l.minutes=a.minutes)`;
const costWitness = `NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations c WHERE c.id=a.source_id AND c.project_id=a.project_id AND c.company_id=a.company_id AND c.version=a.source_version AND c.state='approved' AND c.amount_minor=a.amount_minor)`;
const workLogArm = `UNION ALL SELECT 'work-log',l.id,l.version,l.state,p.id IS NULL OR l.state='submitted' OR NOT EXISTS
 (SELECT 1 FROM staff_project_work_log_events event JOIN command_receipts command ON command.record_id=l.id
 AND command.operation='project.actual-time.'||CASE WHEN l.state='approved' THEN 'approve' ELSE 'reject' END
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=l.company_id
 AND command.result_json::jsonb->>'id'=l.id AND command.result_json::jsonb->>'version'=l.version::text
 AND command.result_json::jsonb->>'state'=l.state WHERE event.work_log_id=l.id AND event.work_log_version=l.version AND event.action=l.state)
 OR (l.state='approved' AND NOT EXISTS(SELECT 1 FROM staff_project_package_actuals actual WHERE actual.source_kind='work_log' AND actual.source_id=l.id
 AND actual.source_version=l.version AND actual.company_id=l.company_id AND actual.project_id=l.project_id AND actual.minutes=l.minutes))
 FROM staff_project_work_logs l CROSS JOIN scope LEFT JOIN projects p ON p.id=l.project_id WHERE scope.is_base AND l.company_id=scope.company_id`;
const costArm = `UNION ALL SELECT 'cost-allocation',c.id,c.version,c.state,p.id IS NULL OR c.state='draft' OR NOT EXISTS
 (SELECT 1 FROM staff_project_cost_allocation_events event JOIN command_receipts command ON command.record_id=c.id
 AND command.operation='project.cost-allocation.'||CASE WHEN c.state='approved' THEN 'approve' ELSE 'reject' END
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=c.company_id
 AND command.result_json::jsonb->>'id'=c.id AND command.result_json::jsonb->>'version'=c.version::text
 AND command.result_json::jsonb->>'state'=c.state WHERE event.allocation_id=c.id AND event.allocation_version=c.version AND event.action=c.state)
 OR (scope.is_base AND c.state='approved' AND NOT EXISTS(SELECT 1 FROM staff_project_package_actuals actual WHERE actual.source_kind='cost_allocation'
 AND actual.source_id=c.id AND actual.source_version=c.version AND actual.company_id=c.company_id AND actual.project_id=c.project_id AND actual.amount_minor=c.amount_minor))
 FROM staff_project_cost_allocations c CROSS JOIN scope LEFT JOIN projects p ON p.id=c.project_id WHERE c.company_id=scope.company_id`;

const workLogArgs = {
  kind: 'work-log', alias: 'l', state: 'submitted', table: 'staff_project_work_log_events', op: 'project.actual-time.',
  fkId: 'work_log_id', fkVer: 'work_log_version', actualKind: 'work_log', measure: 'minutes',
  kindGap: ' ', idGap: '\n ', scopeGuard: '', relation: 'staff_project_work_logs', whereScope: 'scope.is_base AND ',
};
const costArgs = {
  kind: 'cost-allocation', alias: 'c', state: 'draft', table: 'staff_project_cost_allocation_events', op: 'project.cost-allocation.',
  fkId: 'allocation_id', fkVer: 'allocation_version', actualKind: 'cost_allocation', measure: 'amount_minor',
  kindGap: '\n ', idGap: ' ', scopeGuard: 'scope.is_base AND ', relation: 'staff_project_cost_allocations', whereScope: '',
};

describe('Projects read-tail r36 package-actual source and event-fact arm dedupe', () => {
  test('packageActualSourceProof reproduces both former inline NOT EXISTS witnesses byte for byte', () => {
    expect(packageActualSourceProof('staff_project_work_logs', 'l', 'minutes')).toBe(workLogWitness);
    expect(packageActualSourceProof('staff_project_cost_allocations', 'c', 'amount_minor')).toBe(costWitness);
    for (const witness of [workLogWitness, costWitness]) {
      expect(witness.startsWith('NOT EXISTS(SELECT 1 FROM ')).toBe(true);
      expect(witness.endsWith(')')).toBe(true);
      expect(witness).not.toContain('?'); // zero binds
      expect(witness.match(/SELECT /g)).toHaveLength(1);
    }
  });

  test('eventFactArm reproduces both former inline UNION arms byte for byte', () => {
    expect(eventFactArm(workLogArgs)).toBe(workLogArm);
    expect(eventFactArm(costArgs)).toBe(costArm);
    expect(workLogArm).not.toBe(costArm);
    for (const arm of [workLogArm, costArm]) {
      expect(arm.startsWith("UNION ALL SELECT '")).toBe(true);
      expect(arm.match(/SELECT /g)).toHaveLength(3); // arm head + event proof + package-actual witness
      expect(arm.split('\n').slice(1).every(line => line.startsWith(' '))).toBe(true);
    }
    // The two whitespace gaps are opposite between the arms and are preserved.
    expect(workLogArm).toContain("actual.source_kind='work_log' AND actual.source_id=l.id\n AND actual.source_version");
    expect(costArm).toContain("actual.source_kind='cost_allocation'\n AND actual.source_id=c.id AND actual.source_version");
    // Nested helpers still emit their own verbatim arms.
    expect(workLogArm).toContain(projectReceiptEventProof('staff_project_work_log_events', 'l', 'project.actual-time.', 'work_log_id', 'work_log_version'));
    expect(costArm).toContain(projectPackageActualProof('cost_allocation', 'c', 'amount_minor', '\n ', ' '));
  });

  test('both call sites compose the helpers and the standalone literals are gone', () => {
    const obl = src('package-obligations.ts');
    expect(obl).toContain('export const packageActualSourceProof=');
    expect(obl).toContain('export const eventFactArm=');
    expect(obl).toContain("${eventFactArm({kind:'work-log',alias:'l'");
    expect(obl).toContain("${eventFactArm({kind:'cost-allocation',alias:'c'");
    expect(obl).toContain("${packageActualSourceProof('staff_project_work_logs','l','minutes')}");
    expect(obl).toContain("${packageActualSourceProof('staff_project_cost_allocations','c','amount_minor')}");
    expect(obl).not.toContain("NOT EXISTS(SELECT 1 FROM staff_project_work_logs l WHERE l.id=a.source_id");
    expect(obl).not.toContain("NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations c WHERE c.id=a.source_id");
    expect(obl).not.toContain("UNION ALL SELECT 'work-log',l.id,l.version");
    expect(obl).not.toContain("UNION ALL SELECT 'cost-allocation',c.id,c.version");
  });

  test('locks, spaced JOIN constants, FOR clauses and native Rust stay untouched', () => {
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
    expect(src('task-purchases.ts')).toContain("${lock?' FOR UPDATE':''}");
    expect(src('executive-evidence-chains.ts')).toContain('JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id');
    // Prior r2-r35 helpers remain present.
    for (const [file, symbol] of [
      ['read-tail-r23.ts', 'projectAssignmentAuthorityReadSql'],
      ['read-tail-r27.ts', 'projectMemberExistsSql'],
      ['read-tail-r28.ts', 'assignmentViewerSelect'],
      ['read-tail-r29.ts', 'orderedImageAggregateHead'],
      ['read-tail-r30.ts', 'visibleProjectRowsHead'],
      ['read-tail-r32.ts', 'projectPurchaseSourceHead'],
      ['read-tail-r33.ts', 'projectPointReadHead'],
      ['read-tail-r34.ts', 'staffTaskPointHead'],
    ] as const) expect(src(file)).toContain(symbol);
    // Native owner sources are reference material only and are not edited here.
    expect(readFileSync(resolve(srcRoot, 'adapters/project_actuals.rs'), 'utf8')).toContain('FROM staff_project_cost_allocations');
  });

  test('retirement:ts-statements is 93 after r36 (BEFORE=95)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r35 left 95. r36 Rank1 folds the two package-actual source NOT EXISTS
    // witnesses into one helper (96-in-file -> 95-in-file, -1) and Rank2 folds
    // the work-log/cost-allocation UNION arms into one closed helper (-1).
    expect(total).toBe(93);
    expect(total).toBeLessThan(95);
  });
});
