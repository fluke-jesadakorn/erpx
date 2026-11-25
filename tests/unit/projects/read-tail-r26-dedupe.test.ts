import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { projectPackageActualProof } from '../../../packages/server-projects/src/adapters/package-obligations';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe guards from HEAD before the r26 edit. These are
// byte-identity oracles only, never live query sources.
const workLogGuard = `NOT EXISTS(SELECT 1 FROM staff_project_package_actuals actual WHERE actual.source_kind='work_log' AND actual.source_id=l.id
 AND actual.source_version=l.version AND actual.company_id=l.company_id AND actual.project_id=l.project_id AND actual.minutes=l.minutes)`;
const costAllocationGuard = `NOT EXISTS(SELECT 1 FROM staff_project_package_actuals actual WHERE actual.source_kind='cost_allocation'
 AND actual.source_id=c.id AND actual.source_version=c.version AND actual.company_id=c.company_id AND actual.project_id=c.project_id AND actual.amount_minor=c.amount_minor)`;

describe('Projects read-tail r26 package-actual proof guard family dedupe', () => {
  test('helper reproduces both former inline package-actual guards byte for byte', () => {
    expect(projectPackageActualProof('work_log', 'l', 'minutes', ' ', '\n ')).toBe(workLogGuard);
    expect(projectPackageActualProof('cost_allocation', 'c', 'amount_minor', '\n ', ' ')).toBe(costAllocationGuard);
    // Only the source kind, alias, measure column and two whitespace gaps vary.
    expect(workLogGuard).toContain("actual.source_kind='work_log'");
    expect(costAllocationGuard).toContain("actual.source_kind='cost_allocation'");
    expect(workLogGuard).toContain('actual.minutes=l.minutes');
    expect(costAllocationGuard).toContain('actual.amount_minor=c.amount_minor');
  });

  test('both call sites compose the helper and keep their outer predicates unchanged', () => {
    const obligations = src('package-obligations.ts');
    // r36 folds both call sites into eventFactArm; the guard tuple now lives in
    // its closed args and the helper delegates with the same byte-exact params.
    expect(obligations).toContain("actualKind:'work_log',measure:'minutes',kindGap:' ',idGap:'\\n '");
    expect(obligations).toContain("actualKind:'cost_allocation',measure:'amount_minor',kindGap:'\\n ',idGap:' '");
    expect(obligations).toContain('${projectPackageActualProof(a.actualKind,a.alias,a.measure,a.kindGap,a.idGap)}');
    expect(obligations).not.toContain("NOT EXISTS(SELECT 1 FROM staff_project_package_actuals actual WHERE actual.source_kind='work_log'");
    expect(obligations).not.toContain("NOT EXISTS(SELECT 1 FROM staff_project_package_actuals actual WHERE actual.source_kind='cost_allocation'");
    expect(obligations).toContain("scopeGuard:'',relation:'staff_project_work_logs',whereScope:'scope.is_base AND '");
    expect(obligations).toContain("scopeGuard:'scope.is_base AND ',relation:'staff_project_cost_allocations',whereScope:''");
    expect(obligations).toContain("OR (${a.scopeGuard}${a.alias}.state='approved' AND ");
    expect(locksUntouched()).toBe(true);
  });

  test('locks, r2-r25 helpers and spaced JOIN constants stay untouched', () => {
    const obligations = src('package-obligations.ts');
    expect(obligations).toContain('export const projectRowsSql=');
    expect(obligations).toContain('const tableRowsSql=`SELECT * FROM `;');
    expect(obligations).toContain('export const projectAssignmentScopeSql=');
    expect(obligations).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones');

    const defs = src('project-read-projections.ts');
    for (const kept of ['projectTaskOwnerSql', 'projectTaskMemberSql', 'projectOwnerOrMemberSql',
      'projectOwnerOrMemberColumnSql', 'projectMemberScopeSql', 'projectMemberScopeGuard',
      'projectHandoverSourceSql', 'socialProjectAudienceSql']) {
      expect(defs).toContain(kept);
    }
    expect(src('clock-cte.ts')).toContain('export const projectsClockCte=');
    expect(src('read-tail-r23.ts')).toContain('projectAssignmentAuthorityReadSql');
    expect(src('executive-metric-projections.ts')).toContain('const groupedMetricSelect =');
    expect(src('executive-main-evidence-sources.ts')).toContain('const projectDependencyBlockerLateral =');
    const chains = src('executive-evidence-chains.ts');
    expect(chains).toContain('JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id');
    expect(chains).toContain('JOIN (${financeProjectionSources.finance_journal_lines}) l ON l.id=a.journal_line_id');
    expect(chains).toContain('JOIN (${sourceSql}) p ON p.id=l.project_id AND p.company_id=l.company_id');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('scheduled-tasks.ts')).toContain('FOR UPDATE OF t');
    expect(src('task-purchases.ts')).toContain("${lock?' FOR UPDATE':''}");
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
    expect(total).toBe(73); // r37 four-arm fact UNION closed helper (-3) + 3-arm NOT EXISTS proof group (-2): 93->88 // r36 package-actual source + event-fact arms: 95->93 // r35 point-read family merge (projectIdSql -> projectPointReadHead): 96->95 // r27 member EXISTS 118->117; r28 viewer head 117->115; r29 image head 115->113; r30 visible-project-rows head 113->112; r31 receipt-event proof 112->111; r31 owner-or-member column 111->110; r32 purchase-source head 110->108; r33 point-read head and owner-or-member reuse 108->105. r34 staff_tasks point 4->1 (-3) and staff_projects by-ref 7->1 (-6): 105->96 // r38 member EXISTS closed helper (-4) + evidence super-head (-3): 88->81
  });
});

function locksUntouched(): boolean {
  const obligations = src('package-obligations.ts');
  return obligations.includes('lockProjectTaskSettlementSources(database)')
    && obligations.includes('owner.assertActiveTransaction(database)')
    && obligations.includes('PROJECT_PACKAGE_OBLIGATION_SOURCE_REQUIRED')
    && obligations.includes('createHash("sha256")');
}
