import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { finalRootOwnerProjectionSources as collaborationProjectionSources } from '@erpx/server-collaboration/public/final-root-owner-projection-sources';
import { projectHandoverSourceSql } from '@erpx/server-projects/public/project-read-projections';
import { operationsProjectSourceExists } from '../../../packages/server-operations/src/adapters/project-source-exists';
import { operationsJoinSourceExists } from '../../../packages/server-operations/src/adapters/join-source-exists';
import { operationsActiveDelegationSql } from '../../../packages/server-operations/src/adapters/task-delegation-source';
import { operationsSocialTaskOriginExists } from '../../../packages/server-operations/src/adapters/social-task-origin-exists';
import { handoverTaskAccessSql, handoverSourcesReadableSql } from '../../../packages/server-operations/src/adapters/handover-policy';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');

const clock = 'floor(extract(epoch FROM clock_timestamp())*1000)::bigint';

describe('operations read-tail r47 EXISTS-family dedupe', () => {
  it('widen project-source probe renders byte-identically for every separator variant', () => {
    // (A1) work-package-obligations social-origin probe, space join.
    expect(operationsProjectSourceExists('§origins', 'o', 'o.task_id=t.id'))
      .toBe('EXISTS(SELECT 1 FROM (§origins) o WHERE o.task_id=t.id)');
    // (A2/A3/A4) no-space joins: retired literals glued the alias to `)`.
    expect(operationsProjectSourceExists('§templates', 'template', 'template.id=t.template_id AND template.company_id=t.company_id AND template.project_id IS NULL AND template.version>=t.template_version', ''))
      .toBe('EXISTS(SELECT 1 FROM (§templates)template WHERE template.id=t.template_id AND template.company_id=t.company_id AND template.project_id IS NULL AND template.version>=t.template_version)');
    expect(operationsProjectSourceExists('§tasks', 't', 't.id=a.record_id AND t.company_id=a.company_id', ''))
      .toBe('EXISTS(SELECT 1 FROM (§tasks)t WHERE t.id=a.record_id AND t.company_id=a.company_id)');
    expect(operationsProjectSourceExists('§exceptionEvents', 'event', 'event.exception_id=e.id AND event.status=e.status AND event.version<=e.version', ''))
      .toBe('EXISTS(SELECT 1 FROM (§exceptionEvents)event WHERE event.exception_id=e.id AND event.status=e.status AND event.version<=e.version)');
    // (A5) staff-policy assignment probe keeps its embedded newline and binds.
    expect(operationsProjectSourceExists('§assignments', 'a', `a.user_id=p.user_id AND a.company_id=p.company_id
    AND a.revoked_at IS NULL AND iam_assignment_active(a.id,${clock}) AND iam_principal_active(a.user_id,a.company_id,${clock}) AND a.valid_from<=? AND a.valid_until>?`))
      .toBe(`EXISTS(SELECT 1 FROM (§assignments) a WHERE a.user_id=p.user_id AND a.company_id=p.company_id
    AND a.revoked_at IS NULL AND iam_assignment_active(a.id,${clock}) AND iam_principal_active(a.user_id,a.company_id,${clock}) AND a.valid_from<=? AND a.valid_until>?)`);
    // (A6) routine-source project probe.
    expect(operationsProjectSourceExists('§staff_projects', 'routine_project', "routine_project.id=t.project_id AND routine_project.company_id=t.company_id"))
      .toBe('EXISTS(SELECT 1 FROM (§staff_projects) routine_project WHERE routine_project.id=t.project_id AND routine_project.company_id=t.company_id)');
    // (A7) handover source probe carries the newline gap after `EXISTS(`.
    expect(operationsProjectSourceExists('§social_post_sources', 'hs', 'hs.post_id=hp.id', ' ', '\n    '))
      .toBe('EXISTS(\n    SELECT 1 FROM (§social_post_sources) hs WHERE hs.post_id=hp.id)');
    // (A8) social-task-origin delegate keeps the exact retired rendering.
    expect(operationsSocialTaskOriginExists('ox', 't.id'))
      .toBe(`EXISTS(SELECT 1 FROM (${collaborationProjectionSources.social_task_origins}) ox WHERE ox.task_id=t.id)`);
    expect(operationsProjectSourceExists('s', 'a', 'p').match(/SELECT /g)).toHaveLength(1);
  });

  it('renders byte-identical handover source readability with the widened probe', () => {
    // Exact retired literal from before the r47 extraction; only `post`/`user` vary.
    const expected = `(hp.purpose<>'handover' OR NOT EXISTS(
    SELECT 1 FROM (${collaborationProjectionSources.social_post_sources}) hs WHERE hs.post_id=hp.id AND (
      (hs.source_kind='staff_task' AND NOT EXISTS(
        SELECT 1 FROM staff_tasks ht WHERE ht.id=hs.source_id AND ht.company_id=hp.company_id
          AND NOT ${operationsSocialTaskOriginExists('ho','ht.id')}
          AND ${handoverTaskAccessSql('ht', 'hu')}))
      OR (hs.source_kind='staff_project' AND NOT EXISTS(${projectHandoverSourceSql('hp', 'hu')}))
    )))`;
    expect(handoverSourcesReadableSql('hp', 'hu')).toBe(expected);
  });

  it('renders the three JOIN probes byte-identically', () => {
    // (B1) executive cohort: origins joined to posts with a newline before WHERE.
    expect(operationsJoinSourceExists('§origins', 'o', '§posts', 'p', 'p.id=o.post_id', 'o.task_id=t.id AND <POSTS>', ' ', '\n          '))
      .toBe('EXISTS(SELECT 1 FROM (§origins) o JOIN (§posts) p ON p.id=o.post_id\n          WHERE o.task_id=t.id AND <POSTS>)');
    // (B2) staff-policy department-head authority.
    expect(operationsJoinSourceExists('§department_heads', 'h', '§organization_nodes', 'n', 'n.id=h.department_id', "h.department_id=? AND h.company_id=? AND h.version=? AND n.status='active'"))
      .toBe("EXISTS(SELECT 1 FROM (§department_heads) h JOIN (§organization_nodes) n ON n.id=h.department_id WHERE h.department_id=? AND h.company_id=? AND h.version=? AND n.status='active')");
    // (B3) work-task-alert pending approval instance/step.
    expect(operationsJoinSourceExists('§instances', 'pending_instance', '§steps', 'pending_step', "pending_step.instance_id=pending_instance.id AND pending_step.state='pending'", "pending_instance.company_id=t.company_id AND pending_instance.record_id=t.id AND pending_instance.state='pending' AND iam_same_person(pending_step.approver_user_id,work_alert_viewer.user_id,t.company_id)"))
      .toBe("EXISTS(SELECT 1 FROM (§instances) pending_instance JOIN (§steps) pending_step ON pending_step.instance_id=pending_instance.id AND pending_step.state='pending' WHERE pending_instance.company_id=t.company_id AND pending_instance.record_id=t.id AND pending_instance.state='pending' AND iam_same_person(pending_step.approver_user_id,work_alert_viewer.user_id,t.company_id))");
    expect(operationsJoinSourceExists('§a', 'a', '§b', 'b', 'x', 'y').match(/SELECT /g)).toHaveLength(1);
  });

  it('delegates the handover active-delegation window byte-identically and preserves all prior callers', () => {
    // (C) exact retired multi-line literal from handoverTaskAccessSql.
    const expected = `SELECT 1 FROM staff_task_delegations hd WHERE hd.task_id=ht.id
      AND hd.company_id=ht.company_id AND hd.status='active'
      AND hd.starts_at<=${clock}
      AND hd.expires_at>${clock}
      AND iam_same_person(hd.delegate_user_id,hu,ht.company_id)`;
    expect(operationsActiveDelegationSql('1', 'hd', 'ht.id', 'ht.company_id', clock, `\n      AND iam_same_person(hd.delegate_user_id,hu,ht.company_id)`, ' ', '\n      AND ')).toBe(expected);
    expect(handoverTaskAccessSql('ht', 'hu')).toContain(`OR EXISTS(${expected})`);
    // Default `pad` leaves the four pre-existing callers unchanged.
    expect(operationsActiveDelegationSql('sd.delegate_user_id', 'sd', 't.id', 't.company_id', '?', '\n      ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1', '\n      '))
      .toBe("SELECT sd.delegate_user_id FROM staff_task_delegations sd WHERE sd.task_id=t.id AND sd.company_id=t.company_id\n      AND sd.status='active' AND sd.starts_at<=? AND sd.expires_at>?\n      ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1");
    expect(operationsActiveDelegationSql('1', 'delegation', 'work_task_file_source.id', 'work_task_file_source.company_id', '${viewer}.now_ms', ' AND iam_same_person(delegation.delegate_user_id,${viewer}.user_id,delegation.company_id)'))
      .toBe("SELECT 1 FROM staff_task_delegations delegation WHERE delegation.task_id=work_task_file_source.id AND delegation.company_id=work_task_file_source.company_id AND delegation.status='active' AND delegation.starts_at<=${viewer}.now_ms AND delegation.expires_at>${viewer}.now_ms AND iam_same_person(delegation.delegate_user_id,${viewer}.user_id,delegation.company_id)");
  });

  it('keeps one definition per family and removes every inline copy from the callers', async () => {
    const [obligations, policy, routine, handover, social, cohort, alert] = await Promise.all([
      src('work-package-obligations.ts'), src('staff-policy.ts'), src('routine-source-read.ts'),
      src('handover-policy.ts'), src('social-task-origin-exists.ts'),
      src('executive-task-cohort.ts'), src('work-task-alert-purchases.ts'),
    ]);
    expect(obligations.match(/operationsProjectSourceExists\(/g)).toHaveLength(4);
    expect(policy.match(/operationsProjectSourceExists\(/g)).toHaveLength(1);
    expect(routine.match(/operationsProjectSourceExists\(/g)).toHaveLength(1);
    expect(handover.match(/operationsProjectSourceExists\(/g)).toHaveLength(1);
    expect(social.match(/operationsProjectSourceExists\(/g)).toHaveLength(1);
    for (const text of [obligations, policy, routine, social]) {
      expect(text).not.toMatch(/EXISTS\(SELECT 1 FROM \(\$\{[^}]+\}\)(?:\s|\))/);
    }
    expect(handover).not.toContain('NOT EXISTS(\n    SELECT 1 FROM (${collaborationProjectionSources.social_post_sources}) hs');
    expect(handover).not.toContain('EXISTS(SELECT 1 FROM staff_task_delegations hd WHERE');
    expect(cohort.match(/operationsJoinSourceExists\(/g)).toHaveLength(1);
    expect(policy.match(/operationsJoinSourceExists\(/g)).toHaveLength(1);
    expect(alert.match(/operationsJoinSourceExists\(/g)).toHaveLength(1);
    expect(cohort).not.toContain('EXISTS(SELECT 1 FROM (${collaborationProjectionSources.social_task_origins}) o JOIN');
    expect(policy).not.toContain('EXISTS(SELECT 1 FROM (${organizationProjectionSources.department_heads}) h JOIN');
    expect(alert).not.toContain('EXISTS(SELECT 1 FROM (${approval.instances}) pending_instance JOIN');
    expect((await src('project-source-exists.ts')).match(/operationsProjectSourceExists/g)).toHaveLength(1);
    expect((await src('join-source-exists.ts')).match(/operationsJoinSourceExists/g)).toHaveLength(1);
  });

  it('leaves LOCK TABLE, command_receipts and native readers untouched', async () => {
    expect(await src('work-package-obligations.ts')).toContain('LOCK TABLE command_receipts IN SHARE MODE');
    expect(await src('work-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(await src('project-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(await src('operations-receipt-version-exists.ts')).toContain('command_receipts');
    expect(await src('work-task-alert-purchases.ts')).toContain('OPERATIONS_RD_WORK_TASK_ALERT_IMAGE');
  });

  it('adds zero new binds and keeps every fragment read-only', () => {
    const all = [
      operationsProjectSourceExists('§a', 'a', 'a.id=b.id'),
      operationsProjectSourceExists('§a', 'a', 'a.id=b.id', ''),
      operationsJoinSourceExists('§a', 'a', '§b', 'b', 'x', 'y'),
      operationsActiveDelegationSql('1', 'hd', 'ht.id', 'ht.company_id', clock, '', ' ', '\n      AND '),
    ].join(' ');
    expect(all).not.toContain('?');
    expect(all).toContain('SELECT 1 FROM');
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i);
  });

  it('retirement:ts-statements reaches the r50 AFTER=82 from BEFORE=105', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(82);
    expect(total).toBeLessThan(105);
  });
});
