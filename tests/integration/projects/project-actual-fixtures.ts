import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/**
 * SQL fixtures for the retained Projects read obligations and executive
 * evidence oracles. Native Rust command tests own the write semantics; these
 * helpers stage already-settled source rows so TypeScript reads keep their
 * coverage without a TypeScript writer.
 */

export type MilestoneSeed = Readonly<{
  id: string;
  companyId: string;
  projectId: string;
  title: string;
  dueAt: number;
  ownerUserId: string;
  createdBy: string;
  description?: string;
  parentId?: string | null;
  ordinal?: number;
  dependsOn?: readonly string[];
  now?: number;
}>;

export async function seedMilestone(db: SqlDatabase, seed: MilestoneSeed) {
  const now = seed.now ?? Date.now();
  const statements = [
    db.prepare(`INSERT INTO staff_project_milestones
      (id,company_id,project_id,title,description,due_at,owner_user_id,parent_milestone_id,ordinal_position,state,version,created_by,completed_at,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,?,?,'planned',1,?,NULL,?,?)`)
      .bind(seed.id, seed.companyId, seed.projectId, seed.title, seed.description ?? '', seed.dueAt, seed.ownerUserId, seed.parentId ?? null, seed.ordinal ?? 0, seed.createdBy, now, now),
  ];
  for (const predecessor of seed.dependsOn ?? [])
    statements.push(
      db.prepare(`INSERT INTO staff_project_milestone_dependencies
        (company_id,project_id,predecessor_milestone_id,successor_milestone_id,created_by,created_at)
        VALUES(?,?,?,?,?,?)`)
        .bind(seed.companyId, seed.projectId, predecessor, seed.id, seed.createdBy, now),
    );
  await db.batch(statements);
  return { id: seed.id, version: 1, state: 'planned' as const };
}

export async function completeMilestone(db: SqlDatabase, input: {
  id: string;
  companyId: string;
  projectId: string;
  version?: number;
  actorId: string;
  now?: number;
}) {
  const version = input.version ?? 1;
  const now = input.now ?? Date.now();
  await db.batch([
    db.prepare(`UPDATE staff_project_milestones
      SET state='completed',completed_at=?,version=?,updated_at=? WHERE id=? AND project_id=? AND company_id=?`)
      .bind(now, version + 1, now, input.id, input.projectId, input.companyId),
    db.prepare(`INSERT INTO staff_project_milestone_events
      (id,milestone_id,milestone_version,action,actor_id,reason,snapshot_json,created_at)
      VALUES(?,?,?,'completed',?,?,?::jsonb,?)`)
      .bind(`${input.id}:${version + 1}`, input.id, version + 1, input.actorId, 'Fixture completion.', '{}', now),
  ]);
  return { id: input.id, version: version + 1, state: 'completed' as const };
}

export async function seedWorkLog(db: SqlDatabase, input: {
  id: string;
  companyId: string;
  projectId: string;
  userId: string;
  workDate: string;
  minutes: number;
  state?: 'submitted' | 'approved';
  version?: number;
  reviewerId?: string;
  now?: number;
}) {
  const now = input.now ?? Date.now();
  const approved = input.state === 'approved';
  await db.prepare(`INSERT INTO staff_project_work_logs
    (id,company_id,project_id,user_id,work_date,minutes,state,version,reviewed_by,reviewed_at,rejection_reason,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,'',?,?)`)
    .bind(
      input.id, input.companyId, input.projectId, input.userId, input.workDate, input.minutes,
      approved ? 'approved' : 'submitted', input.version ?? (approved ? 2 : 1),
      approved ? input.reviewerId ?? null : null, approved ? now : null, now, now,
    )
    .run();
  return { id: input.id, version: input.version ?? (approved ? 2 : 1), state: approved ? 'approved' as const : 'submitted' as const };
}

export async function approveWorkLog(db: SqlDatabase, input: {
  id: string;
  companyId: string;
  projectId: string;
  version: number;
  reviewerId: string;
  receipt: string;
  now?: number;
}) {
  const now = input.now ?? Date.now();
  const version = input.version + 1;
  await db.batch([
    db.prepare(`UPDATE staff_project_work_logs
      SET state='approved',reviewed_by=?,reviewed_at=?,version=?,updated_at=? WHERE id=? AND project_id=? AND company_id=?`)
      .bind(input.reviewerId, now, version, now, input.id, input.projectId, input.companyId),
    db.prepare(`INSERT INTO staff_project_work_log_events
      (id,work_log_id,work_log_version,action,actor_id,reason,snapshot_json,created_at)
      VALUES(?,?,?,'approved',?,?,?::jsonb,?)`)
      .bind(`${input.id}:${version}`, input.id, version, input.reviewerId, 'Fixture approval.', '{}', now),
    db.prepare(`INSERT INTO command_receipts
      (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)
      VALUES(?,'project.actual-time.approve',?,?,?,?,?,?)`)
      .bind(input.receipt, input.reviewerId, 'fixture', input.id, JSON.stringify({ id: input.id, version, state: 'approved' }), now, JSON.stringify({ companyId: input.companyId })),
  ]);
  return { id: input.id, version, state: 'approved' as const };
}

export async function seedCostAllocation(db: SqlDatabase, input: {
  id: string;
  companyId: string;
  projectId: string;
  journalLineId: string;
  amountMinor: number;
  createdBy: string;
  state?: 'draft' | 'approved';
  version?: number;
  approvedBy?: string;
  now?: number;
}) {
  const now = input.now ?? Date.now();
  const approved = input.state === 'approved';
  await db.prepare(`INSERT INTO staff_project_cost_allocations
    (id,company_id,project_id,journal_line_id,amount_minor,state,version,created_by,approved_by,rejected_by,rejection_reason,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?,NULL,'',?,?)`)
    .bind(
      input.id, input.companyId, input.projectId, input.journalLineId, input.amountMinor,
      approved ? 'approved' : 'draft', input.version ?? (approved ? 2 : 1), input.createdBy,
      approved ? input.approvedBy ?? null : null, now, now,
    )
    .run();
  return { id: input.id, version: input.version ?? (approved ? 2 : 1), state: approved ? 'approved' as const : 'draft' as const };
}

export async function approveCostAllocation(db: SqlDatabase, input: {
  id: string;
  companyId: string;
  projectId: string;
  version: number;
  approverId: string;
  receipt: string;
  now?: number;
}) {
  const now = input.now ?? Date.now();
  const version = input.version + 1;
  await db.batch([
    db.prepare(`UPDATE staff_project_cost_allocations
      SET state='approved',approved_by=?,version=?,updated_at=? WHERE id=? AND project_id=? AND company_id=?`)
      .bind(input.approverId, version, now, input.id, input.projectId, input.companyId),
    db.prepare(`INSERT INTO staff_project_cost_allocation_events
      (id,allocation_id,allocation_version,action,actor_id,reason,snapshot_json,created_at)
      VALUES(?,?,?,'approved',?,?,?::jsonb,?)`)
      .bind(`${input.id}:${version}`, input.id, version, input.approverId, 'Fixture approval.', '{}', now),
    db.prepare(`INSERT INTO command_receipts
      (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)
      VALUES(?,'project.cost-allocation.approve',?,?,?,?,?,?)`)
      .bind(input.receipt, input.approverId, 'fixture', input.id, JSON.stringify({ id: input.id, version, state: 'approved' }), now, JSON.stringify({ companyId: input.companyId })),
  ]);
  return { id: input.id, version, state: 'approved' as const };
}
