import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { orderedImageAggregateHead } from '../../../packages/server-projects/src/adapters/read-tail-r29';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe ordered JSONB image heads from HEAD before the r29 edit.
// These are byte-identity oracles only, never live query sources.
const delegationHead = `SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id)`;
const stepHead = `SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id)`;
const memberHead = `SELECT jsonb_agg(to_jsonb(member) ORDER BY member.user_id)`;

describe('Projects read-tail r29 ordered image aggregate head dedupe', () => {
  test('helper reproduces all three former inline heads byte for byte', () => {
    expect(orderedImageAggregateHead('d', 'id')).toBe(delegationHead);
    expect(orderedImageAggregateHead('s', 'id')).toBe(stepHead);
    expect(orderedImageAggregateHead('member', 'user_id')).toBe(memberHead);
    // Alias and ordering column are the only degrees of freedom.
    expect(orderedImageAggregateHead('x', 'created_at')).toBe(
      `SELECT jsonb_agg(to_jsonb(x) ORDER BY x.created_at)`);
    for (const h of [delegationHead, stepHead, memberHead]) {
      expect(h).toMatch(/^SELECT jsonb_agg\(to_jsonb\(\w+\) ORDER BY \w+\.\w+\)$/);
    }
  });

  test('all three call sites compose the helper and keep their outer SQL unchanged', () => {
    const alert = src('scheduled-tasks.ts');
    expect(alert).toContain("import {orderedImageAggregateHead} from './read-tail-r29';");
    expect(alert).toContain("(${orderedImageAggregateHead('d','id')} FROM staff_task_delegations d WHERE d.task_id=t.id");
    expect(alert).toContain("(${orderedImageAggregateHead('s','id')} FROM approval_steps s WHERE s.instance_id=i.id");
    expect(alert).not.toContain('(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM staff_task_delegations');
    expect(alert).not.toContain('(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM approval_steps');

    const social = src('social-purchase-sources.ts');
    expect(social).toContain("import {orderedImageAggregateHead} from './read-tail-r29';");
    expect(social).toContain("(${orderedImageAggregateHead('member','user_id')} FROM staff_project_members member WHERE member.project_id=project.id");
    expect(social).not.toContain('(SELECT jsonb_agg(to_jsonb(member) ORDER BY member.user_id) FROM staff_project_members member WHERE member.project_id=project.id');
  });

  test('locks, r2-r28 helpers and spaced JOIN constants stay untouched', () => {
    const defs = src('project-read-projections.ts');
    for (const kept of ['projectTaskOwnerSql', 'projectTaskMemberSql', 'projectOwnerOrMemberSql',
      'projectOwnerOrMemberColumnSql', 'projectMemberScopeSql', 'projectMemberScopeGuard',
      'projectHandoverSourceSql', 'socialProjectAudienceSql']) {
      expect(defs).toContain(kept);
    }
    expect(defs).toContain('(p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))');
    expect(src('clock-cte.ts')).toContain('export const projectsClockCte=');
    expect(src('read-tail-r23.ts')).toContain('projectAssignmentAuthorityReadSql');
    expect(src('read-tail-r27.ts')).toContain('projectMemberExistsSql');
    expect(src('read-tail-r28.ts')).toContain('assignmentViewerSelect');
    expect(src('package-obligations.ts')).toContain('export const projectPackageActualProof=');
    expect(src('package-obligations.ts')).toContain('export const projectAssignmentScopeSql=');
    expect(src('package-obligations.ts')).toContain('const tableRowsSql=`SELECT * FROM `;');
    expect(src('executive-metric-projections.ts')).toContain('const groupedMetricSelect =');
    expect(src('executive-main-evidence-sources.ts')).toContain('const projectDependencyBlockerLateral =');
    expect(src('final-root-owner-projection-sources.ts')).toContain('const quotedProjection =');
    const chains = src('executive-evidence-chains.ts');
    expect(chains).toContain('JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id');
    expect(chains).toContain('JOIN (${sourceSql}) p ON p.id=l.project_id AND p.company_id=l.company_id');
    // The protected membersJsonSql helper definition is still intact.
    expect(src('social-purchase-sources.ts')).toContain(
      "export const membersJsonSql=`'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM staff_project_members m WHERE m.project_id=p.id)`;");
    // Lock statements and forced-lock reads remain byte-identical.
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('scheduled-tasks.ts')).toContain('FOR UPDATE OF t');
    expect(src('scheduled-tasks.ts')).toContain('FOR SHARE');
    expect(src('task-purchases.ts')).toContain("${lock?' FOR UPDATE':''}");
    expect(untouchedInvariants()).toBe(true);
  });

  test('retirement:ts-statements stays strictly below the r28 BEFORE=115 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(115);
    expect(total).toBe(113); // r28 left 115; r29 ordered-image select-head family 115->113.
  });
});

function untouchedInvariants(): boolean {
  const alert = src('scheduled-tasks.ts');
  return alert.includes("projectsClockCte('project_alert_clock')")
    && alert.includes('SELECT 1 FROM staff_projects project_alert_source CROSS JOIN project_alert_clock')
    && !alert.includes('WITH project_alert_clock AS MATERIALIZED (SELECT')
    && alert.includes("purchasedCapabilityPredicate('project_alert_source.company_id',capability,'settle-existing','project_alert_clock.now_ms')");
}
