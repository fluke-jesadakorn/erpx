import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { OPERATIONS_STAFF_TASK_HEAD, OPERATIONS_TASK_LIST_HEAD } from '../../../packages/server-operations/src/adapters/read-tail-r53-sources';
import { operationsStaffTaskExists } from '../../../packages/server-operations/src/adapters/operations-staff-task-exists';
import { taskParticipantReadPolicy, projectTaskRecordPolicy } from '../../../packages/server-operations/src/adapters/project-task-record-policy';
import { executiveMainEvidenceSources } from '../../../packages/server-operations/src/adapters/executive-main-evidence-sources';
import { projectStructureSources } from '../../../packages/server-operations/src/adapters/project-structure-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const binds = (sql: string) => (sql.match(/\?/g) ?? []).length;

/** Independent pre-r53 byte oracles captured from the removed inline holders. */
const retired = {
  staffTaskExists: 'EXISTS(SELECT 1 FROM staff_tasks work_record_project WHERE work_record_project.id=workspace_records.id AND work_record_project.company_id=workspace_records.company_id AND work_record_project.project_id IS NOT NULL)',
  staffTaskExistsGap: 'EXISTS(\n        SELECT 1 FROM staff_tasks ht WHERE ht.id=hs.source_id AND ht.company_id=hp.company_id)',
  participant: 'b5e1a485afeb5ed11563b7c71b6aedf499860524d1f51b2205ab4780999e55d6',
  projectPolicy: '974369e016af3cda4a8bd616c6bd05e16c2475a756cb7e239607ff4fd4f0e896',
  evidenceSearch: 'daea4d237af9a6fa28dfbdcc5ce7ababf0ec8d4e8f21fa6aa6e57daa729666e7',
  evidenceRead: '9aa01d1cc7096e7add136b598cd1d4ca749c67689c7ffb483684759745f171b1',
  projectStructure: 'SELECT t.id,t.title,t.state,t.version,t.assignee_user_id,b.milestone_id,b.version binding_version\n  FROM staff_tasks t LEFT JOIN staff_project_task_packages b ON b.task_id=t.id WHERE t.project_id=? AND t.company_id=? ORDER BY t.id LIMIT 1001',
};

async function evidenceSql() {
  const sqls: string[] = [];
  const db = {
    prepare(sql: string) {
      sqls.push(sql);
      return { bind() { return this; }, async all() { return { results: [] }; }, async first() { return null; } };
    },
  } as any;
  const actor = { companyId: 'c', branchId: 'b', departmentId: 'd', userId: 'u' } as any;
  const filters = { from: '2026-01-01', to: '2026-01-02', currency: 'USD' };
  const evidence = executiveMainEvidenceSources(db);
  await evidence.factsSearchOperationsEvidence(actor, 1, 'operations-escalations', { filters, query: '', limit: 10, metric: 'operations-escalations' }, null);
  await evidence.factsReadOperationsEvidence(actor, 1, 'open-operations-exceptions', filters, 'tid');
  const structureSqls: string[] = [];
  const structureDb = { prepare(sql: string) { structureSqls.push(sql); return { bind() { return this; }, async all() { return { results: [] }; }, async first() { return null; } }; } } as any;
  await projectStructureSources(structureDb).graphTasks('pid', 'c');
  return { search: sqls[0] ?? '', read: sqls[1] ?? '', structure: structureSqls[0] ?? '' };
}

describe('operations read-tail r53 task/routine head consolidation', () => {
  it('renders the three byte-identical staff-task heads through one owner-local renderer', () => {
    expect(OPERATIONS_STAFF_TASK_HEAD('ht')).toBe('SELECT 1 FROM staff_tasks ht ');
    expect(OPERATIONS_STAFF_TASK_HEAD('project_task_record')).toBe('SELECT 1 FROM staff_tasks project_task_record ');
    expect(OPERATIONS_STAFF_TASK_HEAD('work_record_project')).toBe('SELECT 1 FROM staff_tasks work_record_project ');
    // Default and gap-carrying callers keep the exact retired literals.
    expect(operationsStaffTaskExists('work_record_project', 'workspace_records.id', 'work_record_project.company_id=workspace_records.company_id AND work_record_project.project_id IS NOT NULL'))
      .toBe(retired.staffTaskExists);
    expect(operationsStaffTaskExists('ht', 'hs.source_id', 'ht.company_id=hp.company_id', '\n        '))
      .toBe(retired.staffTaskExistsGap);
    expect(sha(operationsStaffTaskExists('work_record_project', 'workspace_records.id', 'x'))).toBe(sha('EXISTS(SELECT 1 FROM staff_tasks work_record_project WHERE work_record_project.id=workspace_records.id AND x)'));
    // Private task policy readers keep full rendered bytes, binds and ordering.
    const participant = taskParticipantReadPolicy('p', 'c', 'v');
    const policy = projectTaskRecordPolicy('p', 'c', 'v');
    expect(sha(participant)).toBe(retired.participant);
    expect(sha(policy)).toBe(retired.projectPolicy);
    expect(participant.startsWith('EXISTS(SELECT 1 FROM staff_tasks project_task_record LEFT JOIN staff_projects parent_project ON parent_project.id=project_task_record.project_id\n LEFT JOIN LATERAL (SELECT sd.delegate_user_id FROM staff_task_delegations sd WHERE sd.task_id=project_task_record.id')).toBe(true);
    expect(policy).toContain(' AND NOT EXISTS(SELECT 1 FROM social_task_origins origin LEFT JOIN social_posts project_task_post');
    expect(binds(participant)).toBe(0);
    expect(binds(policy)).toBe(0);
  });

  it('renders the two byte-identical task-list projection heads through one renderer', async () => {
    expect(OPERATIONS_TASK_LIST_HEAD).toBe('SELECT t.id,t.title,t.state,t.');
    const { search, read, structure } = await evidenceSql();
    expect(sha(search)).toBe(retired.evidenceSearch);
    expect(sha(read)).toBe(retired.evidenceRead);
    expect(sha(structure)).toBe(sha(retired.projectStructure));
    expect(structure).toBe(retired.projectStructure);
    // Head bytes, bind order and ORDER BY are unchanged in both readers.
    expect(search.startsWith('SELECT t.id,t.title,t.state,t.due_at,t.created_at,t.updated_at,t.version,t.branch_id,t.department_id,\n')).toBe(true);
    expect(read.startsWith('SELECT t.id,t.title,t.state,t.due_at,t.created_at,t.updated_at,t.version,t.branch_id,t.department_id,\n')).toBe(true);
    expect(search).toContain(' ORDER BY t.due_at DESC,t.id DESC LIMIT ?');
    expect(read).toContain(' AND t.id=? LIMIT 1');
    expect(structure).toContain(' ORDER BY t.id LIMIT 1001');
    expect(binds(search)).toBe(25);
    expect(binds(read)).toBe(23);
    expect(binds(structure)).toBe(2);
  });

  it('retires every inline head and keeps one definition per family', async () => {
    const [helper, exists, handover, policy, evidence, structure] = await Promise.all([
      src('read-tail-r53-sources.ts'), src('operations-staff-task-exists.ts'), src('handover.ts'),
      src('project-task-record-policy.ts'), src('executive-main-evidence-sources.ts'), src('project-structure-sources.ts'),
    ]);
    expect(helper.match(/export const OPERATIONS_STAFF_TASK_HEAD/g)).toHaveLength(1);
    expect(helper.match(/export const OPERATIONS_TASK_LIST_HEAD/g)).toHaveLength(1);
    expect(exists.match(/OPERATIONS_STAFF_TASK_HEAD\(/g)).toHaveLength(1);
    expect(handover.match(/OPERATIONS_STAFF_TASK_HEAD\(/g)).toHaveLength(1);
    expect(policy.match(/OPERATIONS_STAFF_TASK_HEAD\(/g)).toHaveLength(1);
    expect(evidence.match(/\$\{OPERATIONS_TASK_LIST_HEAD\}/g)).toHaveLength(1);
    expect(structure.match(/\$\{OPERATIONS_TASK_LIST_HEAD\}/g)).toHaveLength(1);
    for (const text of [exists, handover, policy]) expect(text).not.toContain('SELECT 1 FROM staff_tasks ');
    for (const text of [evidence, structure]) expect(text).not.toContain('SELECT t.id,t.title,t.state,t.');
    expect(handover).toContain('${OPERATIONS_STAFF_TASK_HEAD(\'ht\')}CROSS JOIN (SELECT ?::text user_id) recipient');
    expect(exists).toContain('${OPERATIONS_STAFF_TASK_HEAD(alias)}WHERE ');
    expect(policy).toContain('${OPERATIONS_STAFF_TASK_HEAD(\'project_task_record\')}LEFT JOIN staff_projects');
  });

  it('adds no bind or lock text and leaves protected families untouched', async () => {
    for (const head of [OPERATIONS_STAFF_TASK_HEAD('ht'), OPERATIONS_TASK_LIST_HEAD]) {
      expect(head).not.toContain('?');
      expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    const [alert, native, scope, locks] = await Promise.all([
      src('work-task-alert-purchases.ts'), src('native-operations-read.ts'),
      src('workforce-actor-scope-source.ts'), src('project-source-locks.ts'),
    ]);
    expect(alert).toContain('OPERATIONS_RD_WORK_TASK_ALERT_IMAGE');
    expect(alert).toContain('LOCK TABLE staff_tasks');
    expect(native).toContain('OPERATIONS_RD_');
    expect(scope).toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(locks).toContain('LOCK TABLE staff_tasks IN SHARE ROW EXCLUSIVE MODE');
  });

  it('retirement:ts-statements reaches AFTER=72 from BEFORE=75', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(71);
    expect(total).toBeLessThan(75);
  });
});
