import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { inventoryExecutiveOwnerProjections as financeProjectionSources } from '../../../packages/server-finance/src/public/inventory-executive-owner-projections';
import { inventoryExecutiveOwnerProjections as people_payrollProjectionSources } from '../../../packages/server-people_payroll/src/public/inventory-executive-owner-projections';
import {
  projectActualHoursFrom,
  projectMilestoneOwnerJoin,
  projectMilestoneProjectJoin,
  projectPostedCostFrom,
} from '../../../packages/server-projects/src/adapters/executive-evidence-chains';
import { projectOwnerOrMemberColumnSql, socialProjectAudienceSql } from '../../../packages/server-projects/src/adapters/project-read-projections';
import { socialProjectAudienceReadPredicate } from '../../../packages/server-projects/src/adapters/collaboration-home-sources';
import { projectSocialSourceReadPolicy } from '../../../packages/server-projects/src/adapters/social-purchase-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

describe('Projects read-tail r25 byte-identical FROM/JOIN family dedupe', () => {
  test('milestone chain helpers reproduce the former inline clauses byte for byte', () => {
    expect(projectMilestoneProjectJoin('SRC')).toBe('JOIN (SRC) p ON p.id=m.project_id AND p.company_id=m.company_id');
    expect(projectMilestoneProjectJoin('SRC')).toBe('JOIN (SRC) p ON p.id=m.project_id AND p.company_id=m.company_id');
    expect(projectMilestoneOwnerJoin()).toBe(
      `LEFT JOIN (${people_payrollProjectionSources.hr_people}) h ON h.company_id=m.company_id AND h.user_id=m.owner_user_id AND h.merged_into_id IS NULL`);
  });

  test('posted cost and actual hours chains reproduce the former inline clauses byte for byte', () => {
    expect(projectPostedCostFrom('SRC')).toBe(`FROM staff_project_cost_allocations a JOIN (SRC) p ON p.id=a.project_id AND p.company_id=a.company_id
      JOIN (${financeProjectionSources.finance_journal_lines}) l ON l.id=a.journal_line_id JOIN (${financeProjectionSources.finance_journals}) j ON j.id=l.journal_id
      JOIN (${financeProjectionSources.finance_accounts}) ac ON ac.id=l.account_id`);
    expect(projectActualHoursFrom('SRC')).toBe('FROM staff_project_work_logs l JOIN (SRC) p ON p.id=l.project_id AND p.company_id=l.company_id');
  });

  test('owner-or-member column helper reproduces all four former inline scopes byte for byte', () => {
    expect(projectOwnerOrMemberColumnSql('audience_project', 'social_purchase_actor.user_id', 'm')).toBe(
      '(audience_project.owner_user_id=social_purchase_actor.user_id OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=audience_project.id AND m.user_id=social_purchase_actor.user_id))');
    expect(projectOwnerOrMemberColumnSql('social_project_source', 'social_purchase_actor.user_id', 'member', 'project_id')).toBe(
      '(social_project_source.owner_user_id=social_purchase_actor.user_id OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=social_project_source.project_id AND member.user_id=social_purchase_actor.user_id))');
    expect(projectOwnerOrMemberColumnSql('p', 'project_source_viewer.user_id', 'm')).toBe(
      '(p.owner_user_id=project_source_viewer.user_id OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=project_source_viewer.user_id))');
    expect(projectOwnerOrMemberColumnSql('pr', 'a.user_id', 'pm', 'id', '\n        ')).toBe(`(pr.owner_user_id=a.user_id OR EXISTS(
        SELECT 1 FROM staff_project_members pm WHERE pm.project_id=pr.id AND pm.user_id=a.user_id))`);
  });

  test('public predicates keep their byte-identical rendered SQL', () => {
    expect(socialProjectAudienceReadPredicate()).toContain(
      `      WHERE audience_project.id=p.audience_id AND audience_project.company_id=p.company_id
      AND (audience_project.owner_user_id=social_purchase_actor.user_id OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=audience_project.id AND m.user_id=social_purchase_actor.user_id))
      AND iam_role_allows(`);
    expect(projectSocialSourceReadPolicy()).toContain(
      `(social_project_source.source_kind='staff_project' AND (social_project_source.owner_user_id=social_purchase_actor.user_id OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=social_project_source.project_id AND member.user_id=social_purchase_actor.user_id)) OR social_project_source.source_kind='staff_task' AND EXISTS(`);
    expect(socialProjectAudienceSql('audience')).toBe(`SELECT 1 FROM staff_projects pr WHERE pr.id=audience.audience_id AND pr.company_id=audience.company_id
      AND (pr.owner_user_id=a.user_id OR EXISTS(
        SELECT 1 FROM staff_project_members pm WHERE pm.project_id=pr.id AND pm.user_id=a.user_id))`);
  });

  test('locks, r2-r24 helpers and groupedMetricSelect/projectDependencyBlockerLateral stay untouched', () => {
    const defs = src('project-read-projections.ts');
    for (const kept of ['projectTaskOwnerSql', 'projectTaskMemberSql', 'projectOwnerOrMemberSql',
      'projectMemberScopeSql', 'projectMemberScopeGuard', 'projectHandoverSourceSql']) {
      expect(defs).toContain(kept);
    }
    const sources = src('executive-main-evidence-sources.ts');
    expect(sources).toContain('const projectDependencyBlockerLateral =');
    expect(sources).toContain("SELECT jsonb_agg(jsonb_build_object('id',pr.id,'title',pr.title,'state',pr.state,'version',pr.version) ORDER BY pr.due_at,pr.title) items");
    expect(src('executive-metric-projections.ts')).toContain('const groupedMetricSelect =');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-purchase-sources.ts')).toContain('projectSocialAttachmentSourceReadLocks');
    expect(src('collaboration-home-sources.ts')).toContain('projectsIdTitleOptions');
  });

  test('retirement:ts-statements stays strictly below the r25 BEFORE=122 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(122);
    expect(total).toBe(115); // r25 left 119; r26 proof guards 119->118; r27 member EXISTS 118->117; r28 viewer head 117->115.
  });
});
