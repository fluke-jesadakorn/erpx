import "server-only";
import { createHash } from "node:crypto";
import { hasPermission } from "@/lib/permissions";
import { reportingIssues, type OrganizationPerson, type OrganizationPosition } from "@/lib/organization";
import { positionMoveDate, type PositionMoveInput, type PositionMovePreview, type PositionMoveRecord, type PositionMovePolicyImpact } from "@/lib/position-moves";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import type { PostgresDatabase } from "./runtime/postgres";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { assertion, executeCommand, replayCommand } from "./commands";
import { organizationRoster } from "./organization";
import { recommendationStatements } from "./staff-defaults";
import { readGpoRules, gpoRuleMatchesPosition } from "./gpo-rules";

const hash = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const decode = <T>(value: T | string): T => typeof value === "string" ? JSON.parse(value) as T : value;
const placement = (person: OrganizationPerson) => ({ branchId: person.branch_id, departmentId: person.department_id, levelId: person.level_id, managerPersonId: person.manager_person_id });
const positionPlacement = (position: OrganizationPosition) => ({ branchId: position.branch_id, departmentId: position.department_id, levelId: position.level_id, reportsToPositionId: position.reports_to_position_id });
const scoped = (actor: WorkContext, value: { branch_id: string | null; department_id: string | null }) => (!actor.branchId || actor.branchId === value.branch_id) && (!actor.departmentId || actor.departmentId === value.department_id);
type Leave = { engagement_id: string; id: string; effective_from: string; effective_to: string | null; policy_key: string; version: number };
type LeaveDefault = { level_id: string; policy_version_id: string; effective_from: string; version: number };

/** Validate the projected whole graph, including reports outside the moved cohort. */
export function positionMoveGraphIssues(people: OrganizationPerson[], positions: OrganizationPosition[], candidate: OrganizationPosition, holders: OrganizationPerson[], date: string) {
  const issues: string[] = [];
  const projectedPositions = positions.map(position => position.id === candidate.id ? candidate : position);
  const projectedPeople = people.map(person => holders.find(holder => holder.engagement_id === person.engagement_id) ?? person);
  if (candidate.is_ceo && (candidate.level_number !== 1 || candidate.reports_to_position_id)) issues.push("The CEO position must remain Level 1 without a reporting position.");
  if (!candidate.is_ceo && !candidate.reports_to_position_id) issues.push("Choose a reporting position.");
  for (const position of projectedPositions.filter(item => item.active === 1)) {
    const parent = projectedPositions.find(item => item.id === position.reports_to_position_id && item.active === 1);
    if ((position.id === candidate.id || position.reports_to_position_id === candidate.id) && position.reports_to_position_id && (!parent || parent.level_number >= position.level_number))
      issues.push(`${position.title}: the reporting position must have a more senior Level.`);
    const seen = new Set([position.id]); let next = position.reports_to_position_id;
    while (next) {
      if (seen.has(next)) { issues.push("Correct the circular position reporting relationship."); break; }
      seen.add(next); next = projectedPositions.find(item => item.id === next)?.reports_to_position_id ?? null;
    }
  }
  const holderIds = new Set(holders.map(holder => holder.engagement_id));
  for (const person of projectedPeople) {
    const before = people.find(item => item.engagement_id === person.engagement_id)!;
    const prior = reportingIssues(before, people, positions, date);
    const next = reportingIssues(person, projectedPeople, projectedPositions, date);
    for (const issue of next.filter(issue => holderIds.has(person.engagement_id) || !prior.includes(issue))) issues.push(`${person.legal_name}: ${issue}`);
  }
  return [...new Set(issues)];
}

export function positionMoveLeaveImpact(prior: Leave | undefined, policy: LeaveDefault | undefined, beforeLevelId: string | null, afterLevelId: string, date: string) {
  if (beforeLevelId === afterLevelId) return { leaveEffectiveFrom: null, leaveNote: "Actual Level is unchanged; existing leave remains.", blocker: null };
  if (!policy) return { leaveEffectiveFrom: null, leaveNote: "No leave default is configured for the new Level.", blocker: null };
  if (prior && prior.policy_key !== "legacy-untracked" && !/^level-[1-6]-default$/.test(prior.policy_key))
    return { leaveEffectiveFrom: null, leaveNote: "Custom leave policy is preserved.", blocker: null };
  const effective = beforeLevelId ? `${Number(date.slice(0, 4)) + 1}-01-01` : date < policy.effective_from ? policy.effective_from : date;
  return { leaveEffectiveFrom: effective, leaveNote: `Level leave default starts ${effective}. Salary is unchanged.`, blocker: prior && prior.effective_from >= effective ? "Review the overlapping future leave assignment." : null };
}

async function prepareMove(db: SqlDatabase, actor: WorkContext, input: PositionMoveInput, expiresAt: number, moveId: string | null = null) {
  requirePermission(actor, "organization.manage");
  const { people, positions } = await organizationRoster(db, actor.companyId);
  const position = positions.find(item => item.id === input.positionId && item.active === 1);
  if (!position || !scoped(actor, position)) throw new ApiError(404, "POSITION_NOT_FOUND", "Choose an active position in your authorized workspace.");
  const blockers: string[] = [];
  if (position.version !== input.expectedVersion) blockers.push("The position changed. Reload it and preview again.");
  const nodes = (await db.prepare("SELECT id,kind,branch_id,status,hierarchy_revision FROM organization_nodes WHERE company_id=? ORDER BY id").bind(actor.companyId).all<{ id: string; kind: string; branch_id: string | null; status: string; hierarchy_revision: number }>()).results;
  const levels = (await db.prepare("SELECT id,level_number,active,version FROM staff_levels WHERE company_id=? ORDER BY id").bind(actor.companyId).all<{ id: string; level_number: number; active: number; version: number }>()).results;
  const candidate = { ...position, reports_to_position_id: input.reportsToPositionId === undefined ? position.reports_to_position_id : input.reportsToPositionId };
  if (input.targetKind === "department") {
    const department = nodes.find(node => node.id === input.targetId && node.kind === "department" && node.status === "active");
    if (!department || !nodes.some(node => node.id === department.branch_id && node.kind === "branch" && node.status === "active"))
      throw new ApiError(422, "DEPARTMENT_INVALID", "Choose an active department in an active branch.");
    candidate.branch_id = department.branch_id; candidate.department_id = department.id;
  } else {
    const level = levels.find(level => level.id === input.targetId && level.active === 1 && level.level_number >= 1 && level.level_number <= 6);
    if (!level) throw new ApiError(422, "LEVEL_INVALID", "Choose one of the six active staff Levels.");
    candidate.level_id = level.id; candidate.level_number = level.level_number;
  }
  if (!scoped(actor, candidate)) throw new ApiError(403, "SCOPE_FORBIDDEN", "The destination is outside your authorized workspace.");
  const members = (await db.prepare("SELECT id,person_id,version,status,start_date,end_date,currency FROM hr_engagements WHERE company_id=? AND position_id=? AND status!='ended' ORDER BY id").bind(actor.companyId, position.id).all<{ id: string; person_id: string; version: number; status: string; start_date: string; end_date: string | null; currency: string }>()).results;
  const cohort = people.filter(person => person.position_id === position.id);
  if (members.length) requirePermission(actor, "people.manage");
  if (members.length !== cohort.length) blockers.push("One or more non-ended holders are unavailable. Review their employee records before moving the position.");
  if (new Set(cohort.map(person => person.person_id)).size !== cohort.length || cohort.some(person => people.filter(other => other.person_id === person.person_id).length !== 1))
    blockers.push("Each employee must have exactly one non-ended engagement before a whole-position move.");
  if (cohort.some(person => !scoped(actor, person))) throw new ApiError(403, "SCOPE_FORBIDDEN", "Every position holder must be in your authorized workspace.");
  if (input.managerAssignments.some(item => !cohort.some(person => person.engagement_id === item.engagementId))) blockers.push("Manager changes must reference only the reviewed position holders.");
  const projected = cohort.map(person => ({ ...person,
    ...(input.targetKind === "department" ? { branch_id: candidate.branch_id, department_id: candidate.department_id } : {}),
    ...(input.targetKind === "level" ? { level_id: candidate.level_id, level_number: candidate.level_number } : {}),
    manager_person_id: input.managerAssignments.find(item => item.engagementId === person.engagement_id)?.managerPersonId ?? (input.managerAssignments.some(item => item.engagementId === person.engagement_id) ? null : person.manager_person_id),
  }));
  const date = positionMoveDate(input.effectiveAt, input.timezone);
  blockers.push(...positionMoveGraphIssues(people, positions, candidate, projected, date));
  const overlaps = (await db.prepare(`SELECT c.id FROM people_lifecycle_cases c WHERE c.company_id=? AND c.status IN ('waiting_it','pending_review','scheduled') AND
    (c.definition->>'positionId'=? OR c.engagement_id IN (SELECT id FROM hr_engagements WHERE company_id=? AND position_id=? AND status!='ended')
      OR c.person_id IN (SELECT person_id FROM hr_engagements WHERE company_id=? AND position_id=? AND status!='ended')) ORDER BY c.id`)
    .bind(actor.companyId, position.id, actor.companyId, position.id, actor.companyId, position.id).all<{ id: string }>()).results;
  const pending = await db.prepare(`SELECT id FROM organization_position_moves WHERE company_id=? AND status='scheduled' AND id IS DISTINCT FROM ? AND
    (position_id=? OR EXISTS(SELECT 1 FROM organization_position_move_members h JOIN hr_engagements e ON e.person_id=h.person_id WHERE h.move_id=organization_position_moves.id AND e.company_id=? AND e.position_id=? AND e.status!='ended')) LIMIT 1`)
    .bind(actor.companyId, moveId, position.id, actor.companyId, position.id).first();
  if (overlaps.length || pending) blockers.push("A holder or this position already has an open placement or position move. Cancel or finish it before previewing again.");
  const defaults = (await db.prepare("SELECT DISTINCT ON(d.level_id) d.level_id,d.policy_version_id,d.version,p.effective_from FROM level_leave_defaults d JOIN leave_policy_versions p ON p.id=d.policy_version_id WHERE d.company_id=? ORDER BY d.level_id,d.version DESC").bind(actor.companyId).all<LeaveDefault>()).results;
  const leave = (await db.prepare(`SELECT a.engagement_id,a.id,a.effective_from,a.effective_to,a.version,p.policy_key FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id
    WHERE a.company_id=? AND a.engagement_id IN (SELECT id FROM hr_engagements WHERE company_id=? AND position_id=? AND status!='ended') AND (a.effective_to IS NULL OR a.effective_to>=?) ORDER BY a.engagement_id,a.effective_from DESC,a.id`)
    .bind(actor.companyId, actor.companyId, position.id, date).all<Leave>()).results;
  const impacts = projected.map(person => {
    const before = cohort.find(item => item.engagement_id === person.engagement_id)!;
    const impact = positionMoveLeaveImpact(leave.find(item => item.engagement_id === person.engagement_id), defaults.find(item => item.level_id === person.level_id), before.level_id, person.level_id!, date);
    if (impact.blocker) blockers.push(`${person.legal_name}: ${impact.blocker}`);
    return { engagementId: person.engagement_id, personId: person.person_id, name: person.legal_name, before: placement(before), after: placement(person), leaveEffectiveFrom: impact.leaveEffectiveFrom, leaveNote: impact.leaveNote };
  });
  const rules = (await readGpoRules(db, actor.companyId)).filter(rule => gpoRuleMatchesPosition(rule, position) || gpoRuleMatchesPosition(rule, candidate));
  const policies = new Map<string, PositionMovePolicyImpact>();
  for (const rule of rules) {
    const key = JSON.stringify([rule.connection_id, rule.gpo_id, rule.user_filter_group_id, rule.computer_target_ou_id]);
    const policy = policies.get(key) ?? { connectionId: rule.connection_id, gpoId: rule.gpo_id, userFilterGroupId: rule.user_filter_group_id, computerTargetOuId: rule.computer_target_ou_id, beforeSources: [], afterSources: [] };
    const source = { ruleId: rule.id, kind: rule.target_kind, id: rule.target_id };
    if (gpoRuleMatchesPosition(rule, position)) policy.beforeSources.push(source);
    if (gpoRuleMatchesPosition(rule, candidate)) policy.afterSources.push(source);
    policies.set(key, policy);
  }
  const policyChanges = { added: [...policies.values()].filter(policy => !policy.beforeSources.length), removed: [...policies.values()].filter(policy => !policy.afterSources.length), retained: [...policies.values()].filter(policy => policy.beforeSources.length && policy.afterSources.length) };
  // Reconciliation status can advance without invalidating review; rule semantics cannot.
  const policySnapshot = rules.map(rule => [rule.id, rule.target_kind, rule.target_id, rule.version, rule.revision, rule.connection_id, rule.gpo_id, rule.user_filter_group_id, rule.computer_target_ou_id, rule.active]);
  const snapshotHash = hash({ people, positions, nodes, levels, members, policySnapshot, defaults: input.targetKind === "level" ? defaults : [], leave: input.targetKind === "level" ? leave : [] });
  const preview: PositionMovePreview = { previewToken: hash({ input, snapshotHash, expiresAt }), expiresAt, ready: blockers.length === 0,
    blockers: [...new Set(blockers)], before: positionPlacement(position), after: positionPlacement(candidate), people: impacts, policyChanges, affectedCount: impacts.length, effectiveAt: input.effectiveAt, timezone: input.timezone };
  return { preview, snapshotHash, members, position, candidate };
}

export async function previewPositionMove(db: PostgresDatabase, actor: WorkContext, input: PositionMoveInput): Promise<PositionMovePreview> {
  return db.transaction(async tx => (await prepareMove(tx, actor, input, Date.now() + 15 * 60_000)).preview);
}

export async function submitPositionMove(db: PostgresDatabase, actor: WorkContext, input: PositionMoveInput, previewToken: string, expiresAt: number, key: string) {
  requirePermission(actor, "organization.manage");
  const id = `position-move-${key}`, operation = "organization.position-move.submit", payload = { input, previewToken, expiresAt };
  return db.transaction(async tx => {
    const prior = await replayCommand(tx, actor, key, operation, id, payload); if (prior) return prior;
    if (expiresAt <= Date.now() || expiresAt > Date.now() + 15 * 60_000) throw new ApiError(409, "POSITION_MOVE_PREVIEW_EXPIRED", "Preview the move again before confirming.");
    if (input.effectiveAt < Date.now() - 60_000) throw new ApiError(422, "EFFECTIVE_DATE_PASSED", "Choose an effective time now or in the future.");
    const prepared = await prepareMove(tx, actor, input, expiresAt);
    if (prepared.preview.previewToken !== previewToken) throw new ApiError(409, "POSITION_MOVE_CHANGED", "The reviewed organization changed. Preview the entire move again.");
    if (!prepared.preview.ready) throw new ApiError(422, "POSITION_MOVE_NOT_READY", prepared.preview.blockers.join(" "));
    const now = Date.now(), result = { id, version: 1, status: "scheduled" as const, effectiveAt: input.effectiveAt };
    // Session expiry must not cancel scheduled work; the assignment and current principal are checked at application time.
    const savedActor = { ...actor, authority: undefined, availableAssignments: undefined, assignments: [], workspaces: [] };
    return executeCommand(tx, actor, key, operation, payload, id, 1, result, [
      tx.prepare("INSERT INTO organization_position_moves(id,company_id,position_id,status,effective_at,definition,preview,snapshot_hash,authorizing_assignment_id,actor_json,created_by,created_at,updated_at) VALUES(?,?,?,'scheduled',?,?,?,?,?,?,?,?,?)")
        .bind(id, actor.companyId, input.positionId, input.effectiveAt, JSON.stringify(input), JSON.stringify(prepared.preview), prepared.snapshotHash, actor.assignmentId, JSON.stringify(savedActor), actor.userId, now, now),
      ...prepared.members.map(member => tx.prepare("INSERT INTO organization_position_move_members(move_id,engagement_id,person_id) VALUES(?,?,?)").bind(id, member.id, member.person_id)),
    ]);
  });
}

function record(row: PositionMoveRecord): PositionMoveRecord { return { ...row, definition: decode(row.definition), preview: decode(row.preview) }; }
export async function positionMoves(db: SqlDatabase, actor: WorkContext, id?: string) {
  requirePermission(actor, "organization.manage");
  const rows = (await db.prepare(`SELECT m.id,m.company_id,m.position_id,m.status,m.version,m.effective_at,m.definition,m.preview,m.created_by,m.created_at,m.updated_at,m.applied_at,m.last_error
    FROM organization_position_moves m JOIN organization_positions p ON p.company_id=m.company_id AND p.id=m.position_id
    WHERE m.company_id=? AND (?::text IS NULL OR m.id=?) AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?) ORDER BY m.created_at DESC,m.id LIMIT 100`)
    .bind(actor.companyId, id ?? null, id ?? null, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<PositionMoveRecord>()).results.map(record);
  // Organization-only readers can inspect empty positions, but cannot inspect HR move snapshots.
  const visible = rows.filter(row => !row.preview.affectedCount || hasPermission(actor, "people.manage"));
  if (id) { if (!visible[0]) throw new ApiError(404, "POSITION_MOVE_NOT_FOUND", "Position move not found."); return visible[0]; }
  return visible;
}

export async function cancelPositionMove(db: PostgresDatabase, actor: WorkContext, id: string, expectedVersion: number, key: string) {
  requirePermission(actor, "organization.manage");
  return db.transaction(async tx => {
    const current = await positionMoves(tx, actor, id) as PositionMoveRecord;
    const prior = await replayCommand(tx, actor, key, "organization.position-move.cancel", id, { expectedVersion }); if (prior) return prior;
    if (current.status !== "scheduled" || current.effective_at <= Date.now()) throw new ApiError(409, "POSITION_MOVE_LOCKED", "Only a scheduled move before its effective time can be cancelled.");
    const version = expectedVersion + 1;
    return executeCommand(tx, actor, key, "organization.position-move.cancel", { expectedVersion }, id, version, { id, version, status: "cancelled" }, [
      assertion(tx, key, "move-version", "EXISTS(SELECT 1 FROM organization_position_moves WHERE id=? AND company_id=? AND status='scheduled' AND version=? AND effective_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)", [id, actor.companyId, expectedVersion]),
      tx.prepare("UPDATE organization_position_moves SET status='cancelled',version=?,updated_at=? WHERE id=? AND company_id=?").bind(version, Date.now(), id, actor.companyId),
    ]);
  });
}

/** HR and every holder commit together; GPO workers observe the committed placement afterward. */
export async function drainPositionMoves(db: PostgresDatabase, now = Date.now()) {
  let processed = 0;
  for (let count = 0; count < 50; count++) {
    let currentId: string | null = null;
    try {
      const applied = await db.transaction(async tx => {
        // Serialize the HR commit with GPO provider writes using the same database lock.
        const providerIdle = await tx.prepare("SELECT pg_try_advisory_xact_lock(821039320) acquired").first<{ acquired: boolean }>();
        if (!providerIdle?.acquired) return false;
        const row = await tx.prepare("SELECT * FROM organization_position_moves WHERE status='scheduled' AND effective_at<=? ORDER BY effective_at,id FOR UPDATE SKIP LOCKED LIMIT 1").bind(now)
          .first<PositionMoveRecord & { snapshot_hash: string; actor_json: string; authorizing_assignment_id: string }>();
        if (!row) return false;
        currentId = row.id;
        const actor = decode<WorkContext>(row.actor_json), input = decode<PositionMoveInput>(row.definition), saved = decode<PositionMovePreview>(row.preview);
        const valid = await tx.prepare(`SELECT 1 FROM assignments a WHERE a.id=? AND a.company_id=? AND a.user_id=? AND a.role=? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
          AND iam_role_allows(a.role,a.company_id,'organization.manage') AND (?=0 OR iam_role_allows(a.role,a.company_id,'people.manage'))
          AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?))`)
          .bind(row.authorizing_assignment_id, row.company_id, row.created_by, actor.role, now, now, saved.affectedCount, actor.branchId, actor.departmentId).first();
        if (!valid) throw new ApiError(409, "POSITION_MOVE_AUTHORITY_CHANGED", "The authorizing HR role changed. Review the move again.");
        const prepared = await prepareMove(tx, actor, input, saved.expiresAt, row.id);
        if (!prepared.preview.ready || prepared.snapshotHash !== row.snapshot_hash) throw new ApiError(409, "POSITION_MOVE_REVIEW_CHANGED", prepared.preview.blockers.join(" ") || "Organization, holders or leave defaults changed. Preview the entire move again.");
        const candidate = prepared.candidate;
        await tx.prepare("UPDATE organization_positions SET branch_id=?,department_id=?,level_id=?,reports_to_position_id=?,version=version+1,updated_at=? WHERE company_id=? AND id=?")
          .bind(candidate.branch_id, candidate.department_id, candidate.level_id, candidate.reports_to_position_id, now, row.company_id, candidate.id).run();
        for (const member of prepared.preview.people) {
          const original = prepared.members.find(item => item.id === member.engagementId)!;
          await tx.prepare("UPDATE hr_engagements SET branch_id=?,department_id=?,level_id=?,manager_person_id=?,version=version+1,updated_at=? WHERE company_id=? AND id=?")
            .bind(member.after.branchId, member.after.departmentId, member.after.levelId, member.after.managerPersonId, now, row.company_id, member.engagementId).run();
          // Employment status, start/end dates, salary and pay group are deliberately preserved.
          await tx.prepare("UPDATE employees SET branch_id=?,department_id=? WHERE company_id=? AND person_id=?").bind(member.after.branchId, member.after.departmentId, row.company_id, member.personId).run();
          await tx.prepare("INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(`position-move-history-${row.id}-${member.engagementId}`, row.company_id, member.engagementId, JSON.stringify({ ...member.before, status: original.status, startDate: original.start_date, endDate: original.end_date }), JSON.stringify({ ...member.after, status: original.status, startDate: original.start_date, endDate: original.end_date, effectiveAt: row.effective_at }), row.created_by, input.reason, row.id, now).run();
          if (member.before.levelId !== member.after.levelId) {
            await tx.prepare("INSERT INTO staff_level_history(id,engagement_id,previous_level_id,level_id,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?)")
              .bind(`position-move-level-${row.id}-${member.engagementId}`, member.engagementId, member.before.levelId, member.after.levelId, row.created_by, input.reason, row.id, now).run();
            await tx.batch(await recommendationStatements(tx, actor, { engagementId: member.engagementId, positionId: candidate.id, levelId: member.after.levelId, priorLevelId: member.before.levelId,
              startDate: positionMoveDate(row.effective_at, input.timezone), currency: original.currency, acceptPay: false, acceptLeave: true, commandId: `${row.id}-${member.engagementId}` }));
          }
        }
        await tx.prepare("UPDATE organization_position_moves SET status='applied',version=version+1,applied_at=?,updated_at=?,last_error=NULL WHERE id=?").bind(now, now, row.id).run();
        await tx.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,'organization.position-move.applied','organization_position_move',?,?,?,?)")
          .bind(`audit-${row.id}`, row.id, row.created_by, row.authorizing_assignment_id, row.id, row.version + 1, input.reason, now).run();
        await tx.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,'organization.position-move.applied','organization_position_move',?,?,?,?)")
          .bind(`event-${row.id}`, row.id, row.id, row.version + 1, JSON.stringify({ companyId: row.company_id, id: row.id, positionId: row.position_id, affectedCount: saved.affectedCount, state: "applied", effectiveAt: row.effective_at }), now).run();
        return true;
      });
      if (!applied) break;
      processed++;
    } catch (error) {
      if (!currentId) throw error;
      // Validation failures never retry partially or block unrelated HR work; a fresh preview is required.
      const message = error instanceof ApiError ? `${error.code}: ${error.message}` : "POSITION_MOVE_RETRY_REQUIRED: No changes applied. Review and submit the move again.";
      await db.prepare("UPDATE organization_position_moves SET status='needs_review',version=version+1,last_error=?,updated_at=? WHERE id=? AND status='scheduled'").bind(message, now, currentId).run();
    }
  }
  return processed;
}
