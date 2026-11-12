import {createHash} from 'node:crypto';
import {loadIamAuthority} from '@erpx/server-identity_access/public/iam-store';
import {nextOccurrence,type RoutineRule} from '@erpx/feature-work_projection/lib/routine-calendar';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

/**
 * SQL fixtures for the retained routine reads, run and generation fence.
 * Native `routine_commands` owns template/schedule write semantics; these
 * helpers stage already-settled owner rows (including the native execution
 * authorization a resume captures) so TypeScript coverage no longer depends on
 * a TypeScript writer.
 */

type RoutineActor = Readonly<{
  userId: string;
  assignmentId: string;
  role: string;
  companyId: string;
  branchId?: string | null;
  departmentId?: string | null;
  authority?: {sessionId: string; [key: string]: unknown};
  iam?: Record<string, unknown>;
}>;

export type RoutineTemplateSeed = Readonly<{
  id?: string;
  companyId: string;
  branchId?: string | null;
  departmentId?: string | null;
  title: string;
  description?: string;
  creatorUserId: string;
  assigneeUserId: string;
  projectId?: string | null;
  startApprovers?: readonly string[];
  completionApprovers?: readonly string[];
  published?: boolean;
  version?: number;
  now?: number;
}>;

/** Stage one already-settled routine template row. */
export async function seedRoutineTemplate(db: SqlDatabase, seed: RoutineTemplateSeed) {
  const now = seed.now ?? Date.now();
  const version = seed.version ?? 1;
  const id = seed.id ?? `routine-${crypto.randomUUID()}`;
  await db
    .prepare(
      `INSERT INTO routine_templates
        (id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,project_id,start_approvers_json,completion_approvers_json,published,version,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`,
    )
    .bind(
      id,
      seed.companyId,
      seed.branchId ?? null,
      seed.departmentId ?? null,
      seed.title,
      seed.description ?? '',
      seed.creatorUserId,
      seed.assigneeUserId,
      seed.projectId ?? null,
      JSON.stringify(seed.startApprovers ?? []),
      JSON.stringify(seed.completionApprovers ?? []),
      seed.published === false ? 0 : 1,
      version,
      now,
      now,
    )
    .run();
  return {id, version};
}

export type RoutineScheduleSeed = Readonly<{
  id?: string;
  templateId: string;
  companyId: string;
  frequency?: RoutineRule['frequency'];
  timezone?: string;
  occurrenceTime?: string;
  startDate: string;
  endDate?: string | null;
  weekday?: number | null;
  monthDay?: number | null;
  dueOffsetHours?: number;
  state?: 'active' | 'paused' | 'ended';
  nextOccurrenceAt?: number | null;
  version?: number;
  now?: number;
}>;

/** Stage one already-settled routine schedule row. */
export async function seedRoutineSchedule(db: SqlDatabase, seed: RoutineScheduleSeed) {
  const now = seed.now ?? Date.now();
  const version = seed.version ?? 1;
  const id = seed.id ?? `schedule-${crypto.randomUUID()}`;
  const rule: RoutineRule = {
    frequency: seed.frequency ?? 'daily',
    timezone: seed.timezone ?? 'Asia/Bangkok',
    occurrence_time: seed.occurrenceTime ?? '09:00',
    start_date: seed.startDate,
    end_date: seed.endDate ?? null,
    weekday: seed.weekday ?? null,
    month_day: seed.monthDay ?? null,
  };
  await db
    .prepare(
      `INSERT INTO routine_schedules
        (id,template_id,company_id,frequency,timezone,occurrence_time,start_date,end_date,weekday,month_day,due_offset_hours,state,next_occurrence_at,paused_at,version,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`,
    )
    .bind(
      id,
      seed.templateId,
      seed.companyId,
      rule.frequency,
      rule.timezone,
      rule.occurrence_time,
      rule.start_date,
      rule.end_date,
      rule.weekday,
      rule.month_day,
      seed.dueOffsetHours ?? 24,
      seed.state ?? 'paused',
      seed.nextOccurrenceAt ?? null,
      seed.state === 'paused' ? now : null,
      version,
      now,
      now,
    )
    .run();
  return {id, version, rule};
}

/**
 * Stage the `work_routine_execution_authorizations` row a native resume
 * captures. The retained generation fence hashes the exact stored template and
 * schedule configuration plus the actor snapshot, so the seed reproduces the
 * same source hash instead of weakening that check.
 */
export async function seedRoutineAuthorization(  db: SqlDatabase,
  actor: RoutineActor,
  seed: {templateId: string; scheduleId: string; kind?: 'create' | 'update' | 'resume'; commandId?: string; now?: number},
) {
  const now = seed.now ?? Date.now();
  const schedule = await db
    .prepare(
      'SELECT version,next_occurrence_at FROM routine_schedules WHERE id=? AND company_id=?',
    )
    .bind(seed.scheduleId, actor.companyId)
    .first<{version: number; next_occurrence_at: number | null}>();
  const template = await db
    .prepare('SELECT version FROM routine_templates WHERE id=? AND company_id=?')
    .bind(seed.templateId, actor.companyId)
    .first<{version: number}>();
  if (!schedule || !template) throw new Error('ROUTINE_SEED_SOURCE_REQUIRED');
  const image = await db
    .prepare(
      `SELECT jsonb_build_object('template',to_jsonb(t),'schedule',to_jsonb(s)-ARRAY['next_occurrence_at','last_generated_at','last_error','updated_at','paused_at','state'])::text image
       FROM routine_templates t
       JOIN routine_schedules s ON s.template_id=t.id AND s.company_id=t.company_id
       WHERE t.id=? AND s.id=? AND t.company_id=?`,
    )
    .bind(seed.templateId, seed.scheduleId, actor.companyId)
    .first<string>('image');
  if (!image) throw new Error('ROUTINE_SEED_SOURCE_REQUIRED');
  const assignmentVersion = await db
    .prepare(
      'SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?',
    )
    .bind(actor.assignmentId, actor.userId, actor.companyId, actor.role)
    .first<number>('version');
  if (!assignmentVersion) throw new Error('ROUTINE_SEED_ASSIGNMENT_REQUIRED');
  const iam =
    actor.iam ??
    (await loadIamAuthority(db, actor.companyId, actor.userId, actor.role, actor.authority?.sessionId));
  const authority = {
    userId: actor.userId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    companyId: actor.companyId,
    branchId: actor.branchId ?? null,
    departmentId: actor.departmentId ?? null,
    authority: actor.authority,
    iam: {...iam, queuedAssignmentVersion: assignmentVersion},
  };
  const commandId = seed.commandId ?? crypto.randomUUID();
  const kind = seed.kind ?? 'resume';
  const operation =
    kind === 'resume'
      ? 'routine.schedule.resume'
      : kind === 'create'
        ? 'routine.schedule.create'
        : 'routine.schedule.update';
  const state = kind === 'resume' ? 'active' : 'paused';
  await db
    .prepare(
      'INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?)',
    )
    .bind(
      commandId,
      operation,
      actor.userId,
      '0'.repeat(64),
      seed.scheduleId,
      JSON.stringify({id: seed.scheduleId, state, version: schedule.version}),
      now,
      JSON.stringify({companyId: actor.companyId}),
    )
    .run();
  await db
    .prepare(
      `INSERT INTO work_routine_execution_authorizations
        (command_id,schedule_id,template_id,company_id,kind,actor_id,assignment_id,authority_json,source_hash,template_version,schedule_version,initial_occurrence_at,created_at)
        VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)`,
    )
    .bind(
      commandId,
      seed.scheduleId,
      seed.templateId,
      actor.companyId,
      kind,
      actor.userId,
      actor.assignmentId,
      JSON.stringify(authority),
      createHash('sha256').update(image).digest('hex'),
      template.version,
      schedule.version,
      schedule.next_occurrence_at,
      now,
    )
    .run();
  return {commandId, sourceHash: createHash('sha256').update(image).digest('hex')};
}

export type RoutineSeedInput = Readonly<{
  title?: string;
  projectId?: string | null;
  published?: boolean;
  assigneeUserId: string;
  startApprovers?: readonly string[];
  completionApprovers?: readonly string[];
  schedule?: {
    frequency?: RoutineRule['frequency'];
    startDate?: string;
    timezone?: string;
    dueOffsetHours?: number;
  } | null;
}>;

type RoutineSeedSchedule = NonNullable<RoutineSeedInput['schedule']>;
type RoutineSeedTemplate = Readonly<{id: string; version: number}>;
type RoutineSeedScheduleRow = Readonly<{id: string; version: number; rule: RoutineRule}>;

/** Stage one settled template and its active resumed schedule. */
export async function seedRoutineFor(
  db: SqlDatabase,
  actor: RoutineActor,
  input: RoutineSeedInput & {schedule: RoutineSeedSchedule},
): Promise<{template: RoutineSeedTemplate; schedule: RoutineSeedScheduleRow; at: number}>;
/** Stage one settled template without a schedule. */
export async function seedRoutineFor(
  db: SqlDatabase,
  actor: RoutineActor,
  input: RoutineSeedInput & {schedule?: null},
): Promise<{template: RoutineSeedTemplate; schedule: null; at: null}>;
export async function seedRoutineFor(
  db: SqlDatabase,
  actor: RoutineActor,
  input: RoutineSeedInput,
) {
  const now = Date.now();
  const template = await seedRoutineTemplate(db, {
    companyId: actor.companyId,
    branchId: actor.branchId ?? null,
    departmentId: actor.departmentId ?? null,
    title: input.title ?? 'Staged routine source',
    creatorUserId: actor.userId,
    assigneeUserId: input.assigneeUserId,
    projectId: input.projectId ?? null,
    startApprovers: input.startApprovers ?? [],
    completionApprovers: input.completionApprovers ?? [],
    published: input.published !== false,
    now,
  });
  const schedule = input.schedule
    ? await seedRoutineSchedule(db, {
        templateId: template.id,
        companyId: actor.companyId,
        frequency: input.schedule.frequency ?? 'daily',
        startDate: input.schedule.startDate ?? '2030-01-01',
        timezone: input.schedule.timezone ?? 'Asia/Bangkok',
        dueOffsetHours: input.schedule.dueOffsetHours ?? 24,
        state: 'active',
        // A native resume captures the incremented schedule version.
        version: 2,
        nextOccurrenceAt: nextOccurrence(
          {
            frequency: input.schedule.frequency ?? 'daily',
            timezone: input.schedule.timezone ?? 'Asia/Bangkok',
            occurrence_time: '09:00',
            start_date: input.schedule.startDate ?? '2030-01-01',
            end_date: null,
            weekday: null,
            month_day: null,
          },
          now - 1,
        ),
        now,
      })
    : null;
  if (schedule)
    await seedRoutineAuthorization(db, actor, {
      templateId: template.id,
      scheduleId: schedule.id,
      kind: 'resume',
      now,
    });
  const at = schedule
    ? await db
        .prepare('SELECT next_occurrence_at FROM routine_schedules WHERE id=?')
        .bind(schedule.id)
        .first<number>('next_occurrence_at')
    : null;
  return {template, schedule, at};
}

/**
 * Stage the settled end of a routine schedule, including the native
 * `routine.schedule.end` receipt that the Work obligations projection requires
 * before it treats an ended schedule as settled.
 */
export async function settleRoutineScheduleEnd(
  db: SqlDatabase,
  seed: {scheduleId: string; companyId: string; actorId: string; commandId?: string; now?: number},
) {
  const now = seed.now ?? Date.now();
  const current = await db
    .prepare('SELECT version FROM routine_schedules WHERE id=? AND company_id=?')
    .bind(seed.scheduleId, seed.companyId)
    .first<number>('version');
  if (!current) throw new Error('ROUTINE_SEED_SCHEDULE_REQUIRED');
  const version = current + 1;
  await db.batch([
    db
      .prepare(
        "UPDATE routine_schedules SET state='ended',next_occurrence_at=NULL,paused_at=NULL,version=?,updated_at=? WHERE id=? AND company_id=?",
      )
      .bind(version, now, seed.scheduleId, seed.companyId),
    db
      .prepare(
        'INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?)',
      )
      .bind(
        seed.commandId ?? crypto.randomUUID(),
        'routine.schedule.end',
        seed.actorId,
        '0'.repeat(64),
        seed.scheduleId,
        JSON.stringify({id: seed.scheduleId, state: 'ended', version}),
        now,
        JSON.stringify({companyId: seed.companyId}),
      ),
  ]);
  return {id: seed.scheduleId, state: 'ended' as const, version};
}

/** Stage the settled unpublish of a routine template (`published=0`). */
export async function settleRoutineUnpublish(
  db: SqlDatabase,
  seed: {templateId: string; companyId: string; now?: number},
) {
  await db
    .prepare(
      'UPDATE routine_templates SET published=0,version=version+1,updated_at=? WHERE id=? AND company_id=?',
    )
    .bind(seed.now ?? Date.now(), seed.templateId, seed.companyId)
    .run();
}

