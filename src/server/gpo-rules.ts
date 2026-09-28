import "server-only";
import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import { gpoRuleInput, gpoRuleRemoval, type GpoRule, type GpoRulePreview, type GpoRuleTarget, type GpoRulesOverview } from "@/lib/gpo-rules";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "./authorization";
import type { PostgresDatabase } from "./runtime/postgres";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { requireFreshMfa } from "./iam-mfa";
import { readAutomationInventory } from "./directory-provider";
import type { ProviderConnection } from "./iam-providers";
import { organizationGuard } from "./organization";

export type GpoRuleRow = {
  id: string; company_id: string; target_kind: GpoRuleTarget["kind"]; target_id: string;
  version: number; revision: number; connection_id: string; gpo_id: string;
  user_filter_group_id: string; computer_target_ou_id: string; active: boolean;
  directory_config_status: GpoRule["directoryStatus"]; directory_config_error: string | null;
  legacy_position_id: string | null; target_name: string;
};
export type EffectiveRuleRow = GpoRuleRow & { position_id: string };
const tokenSchema = z.string().regex(/^[a-f0-9]{64}$/);
const hash = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");
export function canManageGpoRules(actor: WorkContext) {
  return actor.role === "it_admin" && actor.assignmentScope === "holding" && !actor.branchId && !actor.departmentId && hasPermission(actor, "iam.directory.apply");
}
function manage(actor: WorkContext) {
  if (!canManageGpoRules(actor)) throw new ApiError(403, "IT_AUTHORITY_REQUIRED", "Choose Company IT authority to publish organization GPO rules.");
}
async function mutate(actor: WorkContext) {
  manage(actor);
  if (!actor.authority?.sessionId) throw new ApiError(403, "FRESH_MFA_REQUIRED", "Verify MFA before changing Directory access.");
  await requireFreshMfa({ userId: actor.userId, sessionId: actor.authority.sessionId });
}
export async function readGpoRules(db: SqlDatabase, companyId?: string) {
  return (await db.prepare(`SELECT r.*,COALESCE(l.name,n.display_name,p.title,r.target_id) target_name
    FROM organization_gpo_rules r
    LEFT JOIN staff_levels l ON r.target_kind='level' AND l.id=r.target_id AND l.company_id=r.company_id
    LEFT JOIN organization_nodes n ON r.target_kind='department' AND n.id=r.target_id AND n.company_id=r.company_id
    LEFT JOIN organization_positions p ON r.target_kind='position' AND p.id=r.target_id AND p.company_id=r.company_id
    WHERE (?::text IS NULL OR r.company_id=?) ORDER BY r.id`).bind(companyId ?? null, companyId ?? null).all<GpoRuleRow>()).results;
}
/** Position defaults define inheritance. Employee-specific levels never select a rule. */
export async function effectiveGpoRules(db: SqlDatabase, companyId?: string): Promise<EffectiveRuleRow[]> {
  return (await db.prepare(`SELECT r.*,p.id position_id,COALESCE(l.name,n.display_name,direct.title,r.target_id) target_name
    FROM organization_positions p JOIN organization_gpo_rules r ON r.company_id=p.company_id AND r.active
      AND ((r.target_kind='position' AND r.target_id=p.id) OR (r.target_kind='level' AND r.target_id=p.level_id) OR (r.target_kind='department' AND r.target_id=p.department_id))
    LEFT JOIN staff_levels l ON r.target_kind='level' AND l.id=r.target_id AND l.company_id=r.company_id AND l.active=1
    LEFT JOIN organization_nodes n ON r.target_kind='department' AND n.id=r.target_id AND n.company_id=r.company_id AND n.status='active'
    LEFT JOIN organization_positions direct ON r.target_kind='position' AND direct.id=r.target_id AND direct.company_id=r.company_id AND direct.active=1
    WHERE p.active=1 AND (?::text IS NULL OR p.company_id=?)
      AND (r.target_kind='position' OR r.target_kind='level' AND l.id IS NOT NULL OR r.target_kind='department' AND n.id IS NOT NULL)
    ORDER BY p.id,r.id`).bind(companyId ?? null, companyId ?? null).all<EffectiveRuleRow>()).results;
}
function present(row: GpoRuleRow): GpoRule {
  return { id: row.id, target: { kind: row.target_kind, id: row.target_id }, targetName: row.target_name,
    version: row.version, revision: row.revision, connectionId: row.connection_id, gpoId: row.gpo_id,
    userFilterGroupId: row.user_filter_group_id, computerTargetOuId: row.computer_target_ou_id,
    active: row.active, directoryStatus: row.directory_config_status, directoryError: row.directory_config_error,
    legacyPositionId: row.legacy_position_id };
}
export async function gpoRulesOverview(db: PostgresDatabase, actor: WorkContext): Promise<GpoRulesOverview> {
  if (!["iam.read", "people.read", "people.manage", "organization.manage"].some(permission => hasPermission(actor, permission)))
    throw new ApiError(403, "GPO_READ_REQUIRED", "Organization or Directory read access is required.");
  const positions = (await db.prepare("SELECT id FROM organization_positions WHERE company_id=? AND active=1 AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)")
    .bind(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<{ id: string }>()).results;
  const visible = new Set(positions.map(row => row.id));
  const effective = (await effectiveGpoRules(db, actor.companyId)).filter(row => visible.has(row.position_id));
  const visibleRules = new Set(effective.map(row => row.id));
  const rules = (await readGpoRules(db, actor.companyId)).filter(row => row.active && ((!actor.branchId && !actor.departmentId) || visibleRules.has(row.id))).map(present);
  const jobs = (await db.prepare(`SELECT j.id,j.rule_id,j.position_id,j.person_id,j.engagement_id,j.account_id,j.connection_id,
      j.revision,j.desired_action,j.status,j.version,j.last_error,j.directory_observed_at,j.endpoint_observed_at
    FROM position_gpo_jobs j JOIN organization_positions p ON p.id=j.position_id AND p.company_id=j.company_id
    WHERE j.company_id=? AND p.active=1 AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
    ORDER BY j.updated_at DESC,j.id LIMIT 200`)
    .bind(actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<GpoRulesOverview["jobs"][number]>()).results;
  const effectiveByPosition: GpoRulesOverview["effectiveByPosition"] = Object.fromEntries(positions.map(row => [row.id, []]));
  for (const row of effective) effectiveByPosition[row.position_id].push({ ...present(row), inherited: row.target_kind !== "position" });
  return { rules, effectiveByPosition, jobs, canManage: canManageGpoRules(actor) };
}
type Position = { id: string; title: string; branch_id: string | null; department_id: string | null; level_id: string; version: number };
export const gpoRuleMatchesPosition = (rule: Pick<GpoRuleRow, "target_kind" | "target_id" | "active">, position: Pick<Position,"id"|"level_id"|"department_id">) => rule.active &&
  (rule.target_kind === "position" ? rule.target_id === position.id : rule.target_kind === "level" ? rule.target_id === position.level_id : rule.target_id === position.department_id);
const rulesSnapshotSql = `SELECT COALESCE(jsonb_agg(jsonb_build_array(id,target_kind,target_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,active) ORDER BY id),'[]'::jsonb)::text snapshot FROM organization_gpo_rules WHERE company_id=?`;
async function catalogSnapshot(db: PostgresDatabase, companyId: string) { return (await db.prepare(rulesSnapshotSql).bind(companyId).first<{ snapshot: string }>())!.snapshot; }
async function targetName(db: PostgresDatabase, companyId: string, target: GpoRuleTarget) {
  const sql = target.kind === "level" ? "SELECT name FROM staff_levels WHERE company_id=? AND id=? AND active=1 AND level_number IS NOT NULL"
    : target.kind === "department" ? "SELECT display_name name FROM organization_nodes WHERE company_id=? AND id=? AND kind='department' AND status='active'"
      : "SELECT title name FROM organization_positions WHERE company_id=? AND id=? AND active=1";
  const result = await db.prepare(sql).bind(companyId, target.id).first<{ name: string }>();
  if (!result) throw new ApiError(422, "GPO_TARGET_INVALID", "Choose an active Level, Department or Position in this Company.");
  return result.name;
}
async function providerEvidence(db: PostgresDatabase, actor: WorkContext, input: z.infer<typeof gpoRuleInput>) {
  const connection = await db.prepare("SELECT id,provider,config,version FROM iam_connections WHERE company_id=? AND id=? AND status='active'").bind(actor.companyId, input.connectionId).first<ProviderConnection & { version: number }>();
  const state = await db.prepare("SELECT version,tested_version,write_enabled FROM people_directory_state WHERE company_id=? AND connection_id=?").bind(actor.companyId, input.connectionId).first<{ version: number; tested_version: number | null; write_enabled: boolean }>();
  if (!connection || !state?.write_enabled || state.version !== state.tested_version) throw new ApiError(409, "DIRECTORY_WRITE_NOT_READY", "Choose an active, tested Directory connection with writes enabled.");
  const inventory = await readAutomationInventory({ ...connection, config: typeof connection.config === "string" ? JSON.parse(connection.config) : connection.config });
  const objects = [[input.gpoId, "gpo"], [input.userFilterGroupId, "group"], [input.computerTargetOuId, "ou"]].map(([id, kind]) => {
    const object = inventory.objects.find(candidate => candidate.id === id && candidate.kind === kind);
    if (!object) throw new ApiError(422, "DIRECTORY_TARGET_INVALID", `Choose a current ${kind} from this connection.`);
    return { id: object.id, version: object.version };
  });
  return { connectionId: connection.id, connectionVersion: connection.version, stateVersion: state.version, objects };
}
async function impact(db: PostgresDatabase, actor: WorkContext, input: z.infer<typeof gpoRuleInput> | z.infer<typeof gpoRuleRemoval>, removing: boolean) {
  manage(actor);
  const rules = await readGpoRules(db, actor.companyId), current = rules.find(rule => rule.id === input.id);
  if ((input.id && !current) || (current?.version ?? 0) !== input.expectedVersion || (removing && !current?.active))
    throw new ApiError(409, "GPO_RULE_CHANGED", "Reload the current GPO rule before reviewing this change.");
  const definition = input as z.infer<typeof gpoRuleInput>;
  const target = removing ? { kind: current!.target_kind, id: current!.target_id } : definition.target;
  if (current && !removing && (current.target_kind !== target.kind || current.target_id !== target.id)) throw new ApiError(422, "GPO_RULE_TARGET_IMMUTABLE", "Remove the existing assignment and add a rule at the new source.");
  const name = removing ? current!.target_name : await targetName(db, actor.companyId, target);
  const evidence = removing ? null : await providerEvidence(db, actor, definition);
  const candidate = removing ? { ...current!, active: false } : {
    ...current, id: current?.id ?? "new", company_id: actor.companyId, target_kind: target.kind, target_id: target.id,
    connection_id: definition.connectionId, gpo_id: definition.gpoId, user_filter_group_id: definition.userFilterGroupId,
    computer_target_ou_id: definition.computerTargetOuId, active: true,
  } as GpoRuleRow;
  if (!removing && rules.some(rule => rule.active && rule.id !== current?.id && rule.target_kind === target.kind && rule.target_id === target.id && rule.connection_id === candidate.connection_id && rule.gpo_id === candidate.gpo_id && rule.user_filter_group_id === candidate.user_filter_group_id && rule.computer_target_ou_id === candidate.computer_target_ou_id))
    throw new ApiError(409, "GPO_RULE_DUPLICATE", "This GPO mapping is already assigned to this source.");
  const after = [...rules.filter(rule => rule.id !== current?.id), candidate];
  const positions = (await db.prepare("SELECT id,title,branch_id,department_id,level_id,version FROM organization_positions WHERE company_id=? AND active=1 ORDER BY id").bind(actor.companyId).all<Position>()).results;
  const affected = positions.filter(position => gpoRuleMatchesPosition({ ...candidate, active: true }, position));
  const holders = (await db.prepare(`SELECT e.id engagement_id,e.person_id,e.position_id,e.version,p.legal_name name,
    COALESCE((SELECT jsonb_agg(jsonb_build_object('connection',i.connection_id,'id',i.external_id) ORDER BY i.connection_id,i.external_id)
      FROM iam_members m JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id AND i.person_id=m.person_id
      WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active' AND i.enabled=true),'[]'::jsonb) accounts
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
    WHERE e.company_id=? AND e.status='active' AND e.start_date<=CURRENT_DATE::text AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text)
      AND p.status='active' AND p.merged_into_id IS NULL ORDER BY e.id`).bind(actor.companyId)
    .all<{ engagement_id: string; person_id: string; position_id: string; version: number; name: string; accounts: { connection: string; id: string }[] | string }>()).results
    .map(holder => ({ ...holder, accounts: typeof holder.accounts === "string" ? JSON.parse(holder.accounts) as { connection: string; id: string }[] : holder.accounts }));
  const membershipSet = (catalog: GpoRuleRow[]) => new Set(holders.flatMap(holder => {
    const position = positions.find(item => item.id === holder.position_id);
    return position ? catalog.filter(rule => gpoRuleMatchesPosition(rule, position)).flatMap(rule => holder.accounts.filter(account => account.connection === rule.connection_id)
      .map(account => JSON.stringify([rule.connection_id, rule.user_filter_group_id, account.id]))) : [];
  }));
  const beforeSet = membershipSet(rules), afterSet = membershipSet(after);
  const result: Omit<GpoRulePreview, "token" | "expiresAt"> = { target, targetName: name, positions: affected.map(({ id, title }) => ({ id, title })),
    holders: holders.filter(holder => affected.some(position => position.id === holder.position_id)).map(holder => ({ personId: holder.person_id, engagementId: holder.engagement_id, name: holder.name, accountIds: [...new Set(holder.accounts.filter(account => account.connection === candidate.connection_id).map(account => account.id))] })),
    changes: { add: [...afterSet].filter(item => !beforeSet.has(item)), remove: [...beforeSet].filter(item => !afterSet.has(item)), retained: [...beforeSet].filter(item => afterSet.has(item)) },
    inheritedByFuturePositions: target.kind !== "position", endpointStatus: "unconfirmed" };
  const snapshot = await catalogSnapshot(db, actor.companyId);
  return { preview: { ...result, token: hash({ input, result, snapshot, positions, holders, evidence }) }, snapshot, current, evidence };
}
async function issuePreview(db: PostgresDatabase, actor: WorkContext, preview: Omit<GpoRulePreview, "expiresAt">): Promise<GpoRulePreview> {
  const token = hash(randomUUID()), expiresAt = Date.now() + 10 * 60_000;
  await db.prepare("INSERT INTO organization_gpo_previews(token,company_id,user_id,assignment_id,state_token,expires_at) VALUES(?,?,?,?,?,?)")
    .bind(token, actor.companyId, actor.userId, actor.assignmentId, preview.token, expiresAt).run();
  await db.prepare("DELETE FROM organization_gpo_previews WHERE company_id=? AND expires_at<?").bind(actor.companyId, Date.now()).run();
  return { ...preview, token, expiresAt };
}
async function validatePreview(db: PostgresDatabase, actor: WorkContext, token: string, currentStateToken: string) {
  const preview = await db.prepare("SELECT state_token,expires_at FROM organization_gpo_previews WHERE token=? AND company_id=? AND user_id=? AND assignment_id=?")
    .bind(token, actor.companyId, actor.userId, actor.assignmentId).first<{ state_token: string; expires_at: number }>();
  if (!preview || Number(preview.expires_at) <= Date.now()) throw new ApiError(409, "GPO_PREVIEW_EXPIRED", "This preview expired. Review the current impact again.");
  if (preview.state_token !== currentStateToken) throw new ApiError(409, "GPO_PREVIEW_CHANGED", "The organization or Directory impact changed. Review it again.");
}
export async function previewGpoRule(db: PostgresDatabase, actor: WorkContext, raw: unknown) {
  return issuePreview(db, actor, (await impact(db, actor, gpoRuleInput.parse(raw), false)).preview);
}
export async function previewRemoveGpoRule(db: PostgresDatabase, actor: WorkContext, raw: unknown) {
  return issuePreview(db, actor, (await impact(db, actor, gpoRuleRemoval.parse(raw), true)).preview);
}
export async function publishGpoRule(db: PostgresDatabase, actor: WorkContext, raw: unknown, key: string) {
  const parsed = gpoRuleInput.extend({ previewToken: tokenSchema }).parse(raw), { previewToken, ...input } = parsed;
  await mutate(actor);
  const id = input.id ?? `gpo-rule-${key}`, operation = "organization.gpo.publish";
  const prior = await replayCommand(db, actor, key, operation, id, parsed); if (prior) return prior;
  const graphGuard = await organizationGuard(db, actor.companyId, key);
  const result = await impact(db, actor, input, false);
  await validatePreview(db, actor, previewToken, result.preview.token);
  const version = input.expectedVersion + 1, revision = (result.current?.revision ?? 0) + 1, now = Date.now();
  const statements = [db.prepare("SELECT pg_advisory_xact_lock(821039320)"), graphGuard, assertion(db, key, "gpo-rule-catalog", `((${rulesSnapshotSql})::jsonb = ?::jsonb)`, [actor.companyId, result.snapshot])];
  if (result.current?.legacy_position_id) {
    statements.push(db.prepare(`INSERT INTO position_gpo_revisions(id,company_id,position_id,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_gpo_version,affected_person_ids,published_by,reason,command_id,created_at)
      VALUES(?,?,?,?,?,?,?,?,?,?::jsonb,?,?,?,?)`).bind(`position-gpo-${key}`, actor.companyId, result.current.legacy_position_id, revision, input.connectionId, input.gpoId, input.userFilterGroupId, input.computerTargetOuId, result.evidence!.objects[0].version, JSON.stringify(result.preview.holders.map(holder => holder.personId)), actor.userId, input.reason, key, now));
    statements.push(db.prepare("UPDATE position_gpo_policies SET version=?,revision=?,connection_id=?,gpo_id=?,user_filter_group_id=?,computer_target_ou_id=?,directory_config_status='pending',directory_config_error=NULL,updated_at=? WHERE company_id=? AND position_id=?")
      .bind(version, revision, input.connectionId, input.gpoId, input.userFilterGroupId, input.computerTargetOuId, now, actor.companyId, result.current.legacy_position_id));
  } else statements.push(db.prepare(`INSERT INTO organization_gpo_rules(id,company_id,target_kind,target_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,last_actor_id,last_reason,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET version=excluded.version,revision=excluded.revision,connection_id=excluded.connection_id,gpo_id=excluded.gpo_id,user_filter_group_id=excluded.user_filter_group_id,computer_target_ou_id=excluded.computer_target_ou_id,active=true,directory_config_status='pending',directory_config_error=NULL,last_actor_id=excluded.last_actor_id,last_reason=excluded.last_reason,updated_at=excluded.updated_at`)
    .bind(id, actor.companyId, input.target.kind, input.target.id, version, revision, input.connectionId, input.gpoId, input.userFilterGroupId, input.computerTargetOuId, actor.userId, input.reason, now));
  return executeCommand(db, actor, key, operation, parsed, id, version, { id, version, revision, status: "pending", endpointStatus: "unconfirmed" }, statements);
}
export async function removeGpoRule(db: PostgresDatabase, actor: WorkContext, raw: unknown, key: string) {
  const parsed = gpoRuleRemoval.extend({ previewToken: tokenSchema }).parse(raw), { previewToken, ...input } = parsed;
  await mutate(actor);
  const operation = "organization.gpo.remove", prior = await replayCommand(db, actor, key, operation, input.id, parsed); if (prior) return prior;
  const graphGuard = await organizationGuard(db, actor.companyId, key), result = await impact(db, actor, input, true);
  await validatePreview(db, actor, previewToken, result.preview.token);
  const version = input.expectedVersion + 1, revision = result.current!.revision + 1, now = Date.now();
  const statements = [db.prepare("SELECT pg_advisory_xact_lock(821039320)"), graphGuard, assertion(db, key, "gpo-rule-catalog", `((${rulesSnapshotSql})::jsonb = ?::jsonb)`, [actor.companyId, result.snapshot])];
  if (result.current!.legacy_position_id) {
    statements.push(db.prepare(`INSERT INTO position_gpo_revisions(id,company_id,position_id,revision,affected_person_ids,published_by,reason,command_id,created_at) VALUES(?,?,?,?,?::jsonb,?,?,?,?)`)
      .bind(`position-gpo-unmap-${key}`, actor.companyId, result.current!.legacy_position_id, revision, JSON.stringify(result.preview.holders.map(holder => holder.personId)), actor.userId, input.reason, key, now));
    statements.push(db.prepare("UPDATE position_gpo_policies SET version=?,revision=?,connection_id=NULL,gpo_id=NULL,user_filter_group_id=NULL,computer_target_ou_id=NULL,directory_config_status='directory_applied',directory_config_error=NULL,updated_at=? WHERE company_id=? AND position_id=?")
      .bind(version, revision, now, actor.companyId, result.current!.legacy_position_id));
  } else statements.push(db.prepare("UPDATE organization_gpo_rules SET active=false,version=?,revision=?,last_actor_id=?,last_reason=?,updated_at=? WHERE id=? AND company_id=?")
    .bind(version, revision, actor.userId, input.reason, now, input.id, actor.companyId));
  return executeCommand(db, actor, key, operation, parsed, input.id, version, { id: input.id, version, revision, active: false, queuedForReconciliation: true }, statements);
}
export async function retryGpoRuleConfig(db: PostgresDatabase, actor: WorkContext, raw: unknown, key: string) {
  await mutate(actor);
  const input = gpoRuleRemoval.parse(raw);
  return executeCommand(db, actor, key, "organization.gpo.retry", input, input.id, input.expectedVersion, { id: input.id, status: "pending" }, [
    assertion(db, key, "gpo-rule-version", "EXISTS(SELECT 1 FROM organization_gpo_rules WHERE company_id=? AND id=? AND version=? AND active AND directory_config_status='blocked')", [actor.companyId, input.id, input.expectedVersion]),
    db.prepare("UPDATE organization_gpo_rules SET directory_config_status='pending',directory_config_error=NULL WHERE company_id=? AND id=?").bind(actor.companyId, input.id),
    db.prepare("UPDATE position_gpo_policies SET directory_config_status='pending',directory_config_error=NULL WHERE company_id=? AND position_id=(SELECT legacy_position_id FROM organization_gpo_rules WHERE company_id=? AND id=?)").bind(actor.companyId, actor.companyId, input.id),
  ]);
}

// Stable helpers exported for reconciliation and focused migration tests.
export const gpoRuleSourceId = (ruleId: string, engagementId: string, legacy: boolean) => legacy ? `position-gpo:${engagementId}` : `gpo-rule:${JSON.stringify([ruleId, engagementId])}`;
export const newGpoRuleId = () => `gpo-rule-${randomUUID()}`;
