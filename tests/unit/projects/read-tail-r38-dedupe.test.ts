import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { memberExistsSql, evidenceProjectionHead } from '../../../packages/server-projects/src/adapters/read-tail-r38';
import {
  projectReadProjection, projectMemberScopeSql, projectOwnerOrMemberColumnSql,
  projectHandoverSourceSql, projectOwnerOrMemberSql, projectTaskMemberSql, projectMemberScopeGuard,
} from '../../../packages/server-projects/src/adapters/project-read-projections';
import { projectMemberExistsSql } from '../../../packages/server-projects/src/adapters/read-tail-r27';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Verbatim pre-r38 fragments captured from HEAD before the edit.
// Byte-identity oracles only, never live query sources.
const taskMemberOracle = 'EXISTS(SELECT 1 FROM staff_project_members pm WHERE pm.project_id=t.project_id AND pm.user_id=?)';
const memberScopeOracle = 'EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)';
const columnGapOracle = 'EXISTS(\n        SELECT 1 FROM staff_project_members pm WHERE pm.project_id=pr.id AND pm.user_id=a.user_id)';
const handoverOracle = 'EXISTS(SELECT 1 FROM staff_project_members hm WHERE hm.project_id=hp.id AND hm.user_id=u)';
const r27Oracle = 'EXISTS(SELECT 1 FROM staff_project_members ${member} WHERE ${member}.project_id=${project}.id AND ${member}.user_id=?)';

const milestoneOracle = 'SELECT m.id,m.project_id,p.title project_title,m.title,m.due_at,m.owner_user_id,\n      COALESCE(h.legal_name,m.owner_user_id) owner_name,m.state,m.version';
const dependencyOracle = "SELECT m.id,m.project_id,p.title project_title,m.title,m.due_at,m.state,m.version,\n      COALESCE(blockers.items,'[]'::jsonb)::text blockers_json";
const actualHoursOracle = 'SELECT l.id,l.project_id,p.title project_title,l.work_date::text work_date,l.work_date::text sort_date,\n        l.minutes,l.user_id,l.state,l.version';
const postedCostOracle = 'SELECT a.id,a.project_id,p.title project_title,j.entry_date::text entry_date,j.entry_date::text sort_date,\n        j.id journal_id,j.journal_number,j.currency,';

const target = 'SELECT 1 FROM staff_project_members ';

describe('Projects read-tail r38 closed member EXISTS and evidence super-head dedupe', () => {
  test('Rank1 closed helper reproduces every former inline member witness byte for byte', () => {
    // Fixed relation, fixed equality/bind order; only gap/member/project/id/user vary.
    expect(memberExistsSql('', 'pm', 't', 'project_id', '?')).toBe(taskMemberOracle);
    expect(memberExistsSql('', 'm', 'p', 'id', '?')).toBe(memberScopeOracle);
    expect(memberExistsSql('\n        ', 'pm', 'pr', 'id', 'a.user_id')).toBe(columnGapOracle);
    expect(memberExistsSql('', 'hm', 'hp', 'id', 'u')).toBe(handoverOracle);
    expect(memberExistsSql('', '${member}', '${project}', 'id', '?')).toBe(r27Oracle);
    expect(memberExistsSql('', 'member', 'p', 'id', '?').endsWith('member.user_id=?)')).toBe(true);
  });

  test('Rank1 call sites delegate and keep former renders, names and excluded variants', () => {
    expect(projectReadProjection.taskMember).toBe(taskMemberOracle);
    expect(projectMemberScopeSql).toBe(`(p.owner_user_id=? OR ${memberScopeOracle})`);
    expect(projectOwnerOrMemberColumnSql('pr', 'a.user_id', 'pm', 'id', '\n        '))
      .toBe(`(pr.owner_user_id=a.user_id OR ${columnGapOracle})`);
    expect(projectOwnerOrMemberColumnSql('p', '?', 'member'))
      .toBe('(p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=p.id AND member.user_id=?))');
    expect(projectHandoverSourceSql('hs', 'u')).toContain(handoverOracle);
    expect(projectMemberExistsSql()).toBe('EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)');
    expect(projectMemberExistsSql('pr', 'pm')).toBe('EXISTS(SELECT 1 FROM staff_project_members pm WHERE pm.project_id=pr.id AND pm.user_id=?)');

    const defs = src('project-read-projections.ts');
    expect(defs).toContain("import {memberExistsSql} from './read-tail-r38';");
    expect(defs).toContain('"taskMember": memberExistsSql(\'\', \'pm\', \'t\', \'project_id\', \'?\'),');
    expect(defs).toContain("${memberExistsSql('', 'm', 'p', 'id', '?')}");
    expect(defs).toContain("${memberExistsSql(gap, member, project, id, user)}");
    expect(defs).toContain("${memberExistsSql('', 'hm', 'hp', 'id', user)}");
    // iam_same_person variant and the staff_projects JOIN variant are excluded and unchanged.
    expect(projectOwnerOrMemberSql('p')).toContain('iam_same_person(member.user_id,?,p.company_id)');
    expect(projectTaskMemberSql('t', '?')).toContain('FROM staff_projects hpm JOIN staff_project_members hm');
    expect(defs).not.toContain('projectOwnerOrMemberSql(alias:string){return `(iam_same_person(${alias}.owner_user_id,?,${alias}.company_id) OR ${memberExistsSql(');
    expect(projectMemberScopeGuard('m.user_id=?')).toBe('EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND m.user_id=?)');
    const r27 = src('read-tail-r27.ts');
    expect(r27).toContain("import {memberExistsSql} from './read-tail-r38';");
    expect(r27).toContain("memberExistsSql('', member, project, 'id', '?');");
    expect(r27).not.toContain(target + '${member} WHERE');
    // No residual inline member EXISTS outside the closed helper and the two excluded variants.
    // Only the excluded iam_same_person variant still inlines this relation here;
    // projectTaskMemberSql uses a staff_projects JOIN and is also left alone.
    expect((src('project-read-projections.ts').match(/SELECT 1 FROM staff_project_members/g) ?? []).length).toBe(1);
    expect((src('read-tail-r27.ts').match(/SELECT 1 FROM staff_project_members/g) ?? []).length).toBe(0);
    expect((src('read-tail-r38.ts').match(/SELECT 1 FROM staff_project_members/g) ?? []).length).toBe(1);
  });

  test('Rank2 super-head reproduces all four fixed projections and rejects open columns', () => {
    expect(evidenceProjectionHead('milestone')).toBe(milestoneOracle);
    expect(evidenceProjectionHead('dependency')).toBe(dependencyOracle);
    expect(evidenceProjectionHead('actual-hours')).toBe(actualHoursOracle);
    expect(evidenceProjectionHead('posted-cost')).toBe(postedCostOracle);
    for (const head of [milestoneOracle, dependencyOracle, actualHoursOracle, postedCostOracle]) {
      expect(head.startsWith('SELECT ')).toBe(true);
      expect(head).toContain('.project_id,p.title project_title,');
      expect(head.match(/SELECT /g)).toHaveLength(1);
    }
    // Closed union only: an unknown key fails instead of emitting arbitrary columns.
    expect(() => (evidenceProjectionHead as (k: string) => string)('bogus'))
      .toThrow(/Unknown Projects evidence projection/);
    const helper = src('read-tail-r38.ts');
    expect(helper).toContain("export type EvidenceProjectionKind = 'milestone' | 'dependency' | 'actual-hours' | 'posted-cost'");
    expect(helper).toContain('export const evidenceProjectionHead = (kind: EvidenceProjectionKind) => {');
    expect(helper).not.toContain('export const evidenceProjectionHead = (kind: EvidenceProjectionKind, columns: string)');
    expect(helper).toContain('export const memberExistsSql = (gap: string, member: string, project: string, id: string, user: string) =>');
  });

  test('Rank2 const names stay as delegates and rendered SQL matches the HEAD oracles', () => {
    const ev = src('executive-main-evidence-sources.ts');
    expect(ev).toContain("import { evidenceProjectionHead } from './read-tail-r38';");
    expect(ev).toContain("const milestoneEvidenceSelect = evidenceProjectionHead('milestone');");
    expect(ev).toContain("const dependencyEvidenceSelect = evidenceProjectionHead('dependency');");
    expect(ev).toContain("const projectActualHoursEvidenceFrom = (sourceSql: string) => `${evidenceProjectionHead('actual-hours')}");
    expect(ev).toContain("const projectPostedCostEvidenceHead = evidenceProjectionHead('posted-cost');");
    // Four former in-file SELECT heads are gone from this file; the one head now lives in the helper.
    expect((ev.match(/SELECT m\.id,m\.project_id,p\.title project_title/g) ?? []).length).toBe(0);
    expect((ev.match(/SELECT l\.id,l\.project_id,p\.title project_title/g) ?? []).length).toBe(0);
    expect((ev.match(/SELECT a\.id,a\.project_id,p\.title project_title/g) ?? []).length).toBe(0);
    // By-id and paged SQL still compose the fixed heads with the original FROM chains.
    expect(ev).toContain('${projectActualHoursFrom(sourceSql)}');
    expect(ev).toContain('${projectPostedCostFrom(projectSource.sql)}');
    expect(src('executive-evidence-chains.ts'))
      .toContain('JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id');
  });

  test('Rank3: dependency witness keeps its alias/predicate/bind tail; only the closed head moved (r43)', () => {
    // r38 skipped folding the WHOLE witness: the two copies differ in alias,
    // prerequisite join and the search copy's third bind. r43 folds only the
    // closed, bind-free relation head into read-tail-r43; every alias, predicate
    // and bind stays inline. The byte oracles below are the verbatim BEFORE
    // (BEFORE=72) renders, so the narrow fold is provably text-equivalent.
    const HEAD = `SELECT 1 FROM staff_project_milestone_dependencies `;
    const PH = '${projectDependencyRowsHead}';
    const cohortWitness = `AND EXISTS(${PH}dependency
        JOIN staff_project_milestones prerequisite ON prerequisite.id=dependency.predecessor_milestone_id
          AND prerequisite.company_id=dependency.company_id AND prerequisite.project_id=dependency.project_id
        WHERE dependency.company_id=\${project}.company_id AND dependency.project_id=\${project}.id
          AND dependency.successor_milestone_id=\${milestone}.id AND prerequisite.state<>'completed')`;
    const searchWitness = `EXISTS(
      ${PH}search_dependency
      JOIN staff_project_milestones prerequisite ON prerequisite.id=search_dependency.predecessor_milestone_id
      WHERE search_dependency.company_id=p.company_id AND search_dependency.project_id=p.id
        AND search_dependency.successor_milestone_id=m.id AND prerequisite.state<>'completed'
        AND position(lower(?) in lower(prerequisite.title))>0)`;
    expect(src('executive-milestone-cohort.ts')).toContain(cohortWitness);
    const ev = src('executive-main-evidence-sources.ts');
    expect(ev).toContain(searchWitness);
    // Reconstruct the BEFORE render: the sole textual difference is the head token.
    const render = (s: string) => s.split(PH).join(HEAD);
    const cohortRendered = render(cohortWitness);
    const searchRendered = render(searchWitness);
    expect(sha256(cohortRendered)).toBe('526e0cb8b896231ee08212c58e2ff72897257e8e6236b57a78c0b6bd6a8df577');
    expect(Buffer.byteLength(cohortRendered)).toBe(487);
    expect(sha256(searchRendered)).toBe('39a40efa92cbb60857619965427edc49d5180e069c376509441900e450583371');
    expect(Buffer.byteLength(searchRendered)).toBe(442);
    expect(cohortWitness).toContain("JOIN staff_project_milestones prerequisite");
    expect(searchWitness).toContain("prerequisite.state<>'completed'");
    expect(searchRendered.match(/\?/g)).toHaveLength(1);
    expect(cohortRendered.match(/\?/g)).toBeNull();
    // Neither dependency witness was routed through the r38 member helper.
    expect(ev).not.toContain('memberExistsSql');
    expect(src('executive-milestone-cohort.ts')).not.toContain('memberExistsSql');
    // The blocker lateral (a jsonb_agg projection, not an EXISTS) is also left alone.
    expect(ev).toContain('const projectDependencyBlockerLateral = (sourceSql: string) =>');
  });

  test('locks, native sources and prior helpers stay untouched', () => {
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
    for (const [file, symbol] of [
      ['read-tail-r23.ts', 'projectAssignmentAuthorityReadSql'],
      ['read-tail-r27.ts', 'projectMemberExistsSql'],
      ['read-tail-r28.ts', 'assignmentViewerSelect'],
      ['read-tail-r29.ts', 'orderedImageAggregateHead'],
      ['read-tail-r32.ts', 'projectPurchaseSourceHead'],
      ['read-tail-r33.ts', 'projectPointReadHead'],
      ['read-tail-r34.ts', 'staffTaskPointHead'],
      ['read-tail-r37.ts', 'notExistsRow'],
      ['read-tail-r38.ts', 'memberExistsSql'],
    ] as const) expect(src(file)).toContain(symbol);
    expect(readFileSync(resolve(srcRoot, 'adapters/project_actuals.rs'), 'utf8'))
      .toContain('FROM staff_project_cost_allocations');
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
    // r37 left 88. r38 Rank1 folds five member EXISTS witnesses into one closed
    // helper (-4) and Rank2 folds four evidence heads into one closed super-head
    // (-3): 88-7 = 81. Rank3 keeps its tail inline; r43 folds only the closed head.
    expect(total).toBe(66); // lane 27 wave 2 moved the two handoff list/work reads to native (-2) // lane 27 moved the two actuals reads to native Rust (-2)
    expect(total).toBeLessThan(88);
  });
});
