import { readFile } from 'node:fs/promises';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { finalRootOwnerProjectionSources as collaborationProjectionSources } from '@erpx/server-collaboration/public/final-root-owner-projection-sources';
import { projectHandoverSourceSql } from '@erpx/server-projects/public/project-read-projections';
import { operationsTableExists, operationsOwnerVersionExists } from '../../../packages/server-operations/src/adapters/identity-policy-version-guard';
import { obligationSourceHead } from '../../../packages/server-operations/src/adapters/work-package-obligations';
import { operationsStaffTaskExists } from '../../../packages/server-operations/src/adapters/operations-staff-task-exists';
import { operationsSocialTaskOriginExists } from '../../../packages/server-operations/src/adapters/social-task-origin-exists';
import { handoverTaskAccessSql, handoverSourcesReadableSql } from '../../../packages/server-operations/src/adapters/handover-policy';

const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');

describe('operations read-tail r48 thin-alias retirement', () => {
  it('operationsTableExists renders both handbook_articles probes byte-identically', () => {
    // (R1-a) retired original-handbook probe.
    expect(operationsTableExists('handbook_articles original_handbook', 'original_handbook.id=workspace_records.id'))
      .toBe('EXISTS(SELECT 1 FROM handbook_articles original_handbook WHERE original_handbook.id=workspace_records.id)');
    // (R1-b) retired visible probe: the embedded handbook_editors EXISTS stays verbatim in the predicate.
    const predicate = `a.id=workspace_records.id AND a.company_id=workspace_records.company_id AND SCOPE AND ((a.archived=0 AND a.published_revision IS NOT NULL) OR ?::text='holding_owner' OR EXISTS(SELECT 1 FROM handbook_editors editor WHERE editor.company_id=a.company_id AND editor.user_id=?))`;
    const expected = `EXISTS(SELECT 1 FROM handbook_articles a WHERE ${predicate})`;
    expect(operationsTableExists('handbook_articles a', predicate)).toBe(expected);
    // Zero new definition: the alias delegates straight to the existing guard.
    expect(operationsTableExists('handbook_articles a', predicate))
      .toBe(operationsOwnerVersionExists('handbook_articles a', '', predicate));
  });

  it('obligationSourceHead preserves the )t / ) t / s.* CTE head bytes', () => {
    expect(obligationSourceHead('§tasks', 't', 't.*'))
      .toBe('SELECT t.* FROM (§tasks)t');
    expect(obligationSourceHead('§templates', 't', 't.*', ' '))
      .toBe('SELECT t.* FROM (§templates) t');
    expect(obligationSourceHead('§schedules', 's', 's.*'))
      .toBe('SELECT s.* FROM (§schedules)s');
  });

  it('widened operationsStaffTaskExists keeps every r44 caller byte-identical', () => {
    expect(operationsStaffTaskExists('work_record_project', 'workspace_records.id', 'work_record_project.company_id=workspace_records.company_id AND work_record_project.project_id IS NOT NULL'))
      .toBe('EXISTS(SELECT 1 FROM staff_tasks work_record_project WHERE work_record_project.id=workspace_records.id AND work_record_project.company_id=workspace_records.company_id AND work_record_project.project_id IS NOT NULL)');
    expect(operationsStaffTaskExists('work_record_task', 'staff_notifications.record_id', 'work_record_task.project_id IS NULL'))
      .toBe('EXISTS(SELECT 1 FROM staff_tasks work_record_task WHERE work_record_task.id=staff_notifications.record_id AND work_record_task.project_id IS NULL)');
  });

  it('renders the r48 handover probe and the full readability literal byte-identically', () => {
    const handoverProbe = operationsStaffTaskExists('ht', 'hs.source_id', `ht.company_id=hp.company_id
          AND NOT ${operationsSocialTaskOriginExists('ho','ht.id')}
          AND ${handoverTaskAccessSql('ht','hu')}`, '\n        ');
    expect(handoverProbe).toBe(`EXISTS(
        SELECT 1 FROM staff_tasks ht WHERE ht.id=hs.source_id AND ht.company_id=hp.company_id
          AND NOT ${operationsSocialTaskOriginExists('ho','ht.id')}
          AND ${handoverTaskAccessSql('ht','hu')})`);
    // Exact retired literal from before r48; only the widened inner probe changed.
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

  it('retires every inline copy and leaves LOCK TABLE / command_receipts untouched', async () => {
    const [purchases, obligations, staff, handover, guard] = await Promise.all([
      src('record-purchases.ts'), src('work-package-obligations.ts'),
      src('operations-staff-task-exists.ts'), src('handover-policy.ts'),
      src('identity-policy-version-guard.ts'),
    ]);
    expect(purchases.match(/operationsTableExists\(/g)).toHaveLength(2);
    expect(purchases).not.toContain('EXISTS(SELECT 1 FROM handbook_articles');
    expect(obligations.match(/obligationSourceHead\(/g)).toHaveLength(3);
    expect(obligations).not.toContain('MATERIALIZED(SELECT t.* FROM (${operations.tasks})t');
    expect(obligations).not.toContain('MATERIALIZED(SELECT t.* FROM (${operations.templates}) t');
    expect(obligations).not.toContain('MATERIALIZED(SELECT s.* FROM (${operations.schedules})s');
    expect(handover.match(/operationsStaffTaskExists\(/g)).toHaveLength(1);
    expect(handover).not.toContain('SELECT 1 FROM staff_tasks ht WHERE');
    expect(staff.match(/export const operationsStaffTaskExists/g)).toHaveLength(1);
    expect(guard.match(/export const operationsOwnerVersionExists/g)).toHaveLength(1);
    expect(obligations).toContain('LOCK TABLE command_receipts IN SHARE MODE');
    expect(obligations).toContain('operationsReceiptVersionExists(');
  });
});
