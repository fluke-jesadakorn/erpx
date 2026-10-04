import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import type { Row } from "./commerce";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { sha256 } from "./crypto";
import { assistantPage } from "@/lib/ai/workspace-assistant";
import { hasPermission } from "@/lib/permissions";
const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive();
const sourcePath = z.string().max(300).transform((path, ctx) => {
  try { return path === "/" ? path : assistantPage(path).path; }
  catch { ctx.addIssue({ code: "custom", message: "Choose an available assistant source path." }); return z.NEVER; }
});
const scopePath = sourcePath.refine(path => !path.includes("?"), "An assignment scope is a page path.");
const agentSchema = z.object({
  name: z.string().trim().min(2).max(120),
  purpose: reason,
});
const versionSchema = z.object({
  agentId: z.string().min(1),
  prompt: reason,
  allowedSources: z.array(sourcePath).min(1).max(30),
});
const poolSchema = z.object({
  name: z.string().trim().min(2).max(120),
  userIds: z.array(z.string().min(1)).min(1).max(100),
});
const assignSchema = z
  .object({
    agentVersionId: z.string().min(1),
    poolId: z.string().min(1).optional(),
    userId: z.string().min(1).optional(),
    scopePath,
  })
  .refine((x) => Boolean(x.poolId) !== Boolean(x.userId));
const actionSchema = z.object({
  action: z.enum(["approve", "reject", "retire", "revoke"]),
  expectedVersion: version,
  reason,
});
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "ai.manage" : "records.read");
  if (!hasModuleAccess(a.modules, "agent"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Agent governance.",
    );
}
/** A page scope has a segment boundary; /work must never authorize /workforce. */
export function agentSourceAllowed(path: string, allowedSources: readonly string[]) {
  return allowedSources.some(scope => scope === "/" || path === scope ||
    !scope.includes("?") && (path.startsWith(`${scope}/`) || path.startsWith(`${scope}?`)));
}
export type AssistantGovernance = {
  sessionId: string;
  assignmentId: string | null;
  assignmentVersion: number | null;
  agentVersionId: string | null;
  agentVersionRevision: number | null;
  catalogVersion: number | null;
  poolId: string | null;
  poolVersion: number | null;
  prompt: string | null;
  allowedSources: string[];
};
async function assistantPolicy(db: SqlDatabase, a: WorkContext, path: string): Promise<Omit<AssistantGovernance, "sessionId">> {
  const candidates = (await db.prepare(
    `SELECT x.id,x.version assignment_version,x.scope_path,x.pool_id,x.agent_version_id,
      v.version version_revision,v.prompt,v.allowed_sources_json,v.status version_status,
      c.version catalog_version,c.status catalog_status,p.version pool_version,p.status pool_status
     FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id
     JOIN agent_catalog c ON c.id=v.agent_id LEFT JOIN agent_pools p ON p.id=x.pool_id
     WHERE x.company_id=? AND c.company_id=x.company_id AND x.status='active'
       AND (x.user_id=? OR EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL))
     ORDER BY length(x.scope_path) DESC,x.created_at DESC,x.id`,
  ).bind(a.companyId,a.userId,a.userId).all<Row>()).results;
  const row = candidates.find(candidate => agentSourceAllowed(path,[String(candidate.scope_path)]));
  if (!row) return {assignmentId:null,assignmentVersion:null,agentVersionId:null,agentVersionRevision:null,catalogVersion:null,poolId:null,poolVersion:null,prompt:null,allowedSources:["/"]};
  if (row.version_status !== "approved" || row.catalog_status !== "approved" || row.pool_id && row.pool_status !== "active")
    throw new ApiError(409,"AI_AGENT_UNAVAILABLE","This assigned agent is no longer approved or its pool is unavailable. Continue with the manual workflow guide.");
  return {assignmentId:String(row.id),assignmentVersion:Number(row.assignment_version),agentVersionId:String(row.agent_version_id),agentVersionRevision:Number(row.version_revision),catalogVersion:Number(row.catalog_version),poolId:row.pool_id?String(row.pool_id):null,poolVersion:row.pool_id?Number(row.pool_version):null,prompt:String(row.prompt),allowedSources:JSON.parse(String(row.allowed_sources_json)) as string[]};
}
export async function validateAssistantGovernance(db:SqlDatabase,a:WorkContext,path:string,governance:AssistantGovernance) {
  const current=await assistantPolicy(db,a,path),original={...governance};
  delete (original as Partial<AssistantGovernance>).sessionId;
  if(JSON.stringify(current)!==JSON.stringify(original))
    throw new ApiError(409,"AI_AGENT_CHANGED","The approved agent or its assignment changed. Continue manually or ask again.");
  const session=await db.prepare("SELECT 1 FROM ai_private_session_controls c JOIN ai_private_sessions s ON s.id=c.session_id WHERE s.id=? AND s.company_id=? AND s.user_id=? AND s.assignment_id=? AND c.status='active' AND c.expires_at>?")
    .bind(governance.sessionId,a.companyId,a.userId,a.assignmentId,Date.now()).first();
  if(!session)throw new ApiError(409,"AI_SESSION_EXPIRED","This private session expired. Ask again to start a new session; retained evidence stays private.");
}
/** These guards commit with the turn, closing revocation races after provider work. */
export function assistantGovernanceGuards(db:SqlDatabase,a:WorkContext,key:string,path:string,governance:AssistantGovernance):SqlStatement[] {
  const sessionGuards=[db.prepare("SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE").bind(governance.sessionId),
    assertion(db,key,"private-session",`EXISTS(SELECT 1 FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id
      WHERE s.id=? AND s.company_id=? AND s.user_id=? AND s.assignment_id=? AND c.status='active' AND c.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`,[governance.sessionId,a.companyId,a.userId,a.assignmentId])];
  if(!governance.assignmentId)return [...sessionGuards,assertion(db,key,"agent-default",`NOT EXISTS(SELECT 1 FROM agent_assignments x WHERE x.company_id=? AND x.status='active'
    AND (x.scope_path='/' OR ?=x.scope_path OR starts_with(?,x.scope_path||'/') OR starts_with(?,x.scope_path||'?'))
    AND (x.user_id=? OR EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL)))`,[a.companyId,path,path,path,a.userId,a.userId])];
  return [...sessionGuards,db.prepare("SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c").bind(governance.assignmentId),
    ...(governance.poolId?[db.prepare("SELECT id FROM agent_pools WHERE id=? FOR SHARE").bind(governance.poolId)]:[]),
    assertion(db,key,"approved-agent",`EXISTS(SELECT 1 FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id LEFT JOIN agent_pools p ON p.id=x.pool_id
      WHERE x.id=? AND x.company_id=? AND c.company_id=x.company_id AND x.version=? AND x.status='active'
      AND v.id=? AND v.version=? AND v.status='approved' AND c.version=? AND c.status='approved'
      AND (x.user_id=? OR p.id=? AND p.status='active' AND p.version=? AND EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=p.id AND m.user_id=? AND m.revoked_at IS NULL)))`,
    [governance.assignmentId,a.companyId,governance.assignmentVersion,governance.agentVersionId,governance.agentVersionRevision,governance.catalogVersion,a.userId,governance.poolId,governance.poolVersion,a.userId])];
}
export async function agentWorkspace(a: WorkContext) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    [agents, versions, pools, assignments, invocations, people, members, poolHistory, sessions] =
      await Promise.all([
        write
          .prepare(
            "SELECT * FROM agent_catalog WHERE company_id=? ORDER BY updated_at DESC",
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            "SELECT v.*,a.name agent_name FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE a.company_id=? ORDER BY v.created_at DESC",
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare("SELECT * FROM agent_pools WHERE company_id=? ORDER BY name")
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            "SELECT x.*,v.version_number,a.name agent_name FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog a ON a.id=v.agent_id WHERE x.company_id=? ORDER BY x.updated_at DESC",
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            "SELECT * FROM ai_invocation_evidence WHERE company_id=? ORDER BY created_at DESC LIMIT 50",
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            "SELECT DISTINCT x.user_id id,COALESCE(p.legal_name,x.user_id) name FROM assignments x LEFT JOIN hr_people p ON p.company_id=x.company_id AND p.user_id=x.user_id AND p.merged_into_id IS NULL WHERE x.company_id=? AND iam_assignment_active(x.id,?) AND iam_principal_active(x.user_id,x.company_id,?) ORDER BY 2",
          )
          .bind(a.companyId, Date.now(), Date.now())
          .all<Row>(),
        write.prepare("SELECT m.* FROM agent_pool_members m JOIN agent_pools p ON p.id=m.pool_id WHERE p.company_id=? ORDER BY m.pool_id,m.user_id").bind(a.companyId).all<Row>(),
        write.prepare("SELECT r.* FROM agent_pool_revisions r JOIN agent_pools p ON p.id=r.pool_id WHERE p.company_id=? ORDER BY r.created_at DESC").bind(a.companyId).all<Row>(),
        write.prepare("SELECT s.id,s.user_id,s.assignment_id,s.conversation_key,s.generation,s.created_at,c.* FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=? AND (s.user_id=? OR ?) ORDER BY c.updated_at DESC LIMIT 100").bind(a.companyId,a.userId,hasPermission(a,"ai.manage")?1:0).all<Row>(),
      ]);
  return {
    agents: agents.results,
    versions: versions.results,
    pools: pools.results,
    assignments: assignments.results,
    invocations: invocations.results,
    people: people.results,
    members: members.results,
    poolHistory: poolHistory.results,
    sessions: sessions.results.map((session):Row & {effective_status:string}=>({...session,effective_status:session.status==="active"&&Number(session.expires_at)<=Date.now()?"expired":String(session.status)})),
  };
}
export async function createAgent(a: WorkContext, raw: unknown, key: string) {
  access(a, true);
  const i = agentSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `agent-${key}`,
    op = "agent.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = { id, status: "draft", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare("INSERT INTO agent_catalog VALUES(?,?,?,?,'draft',?,NULL,1,?,?)")
      .bind(id, a.companyId, i.name, i.purpose, a.userId, now, now),
  ]);
}
export async function createAgentVersion(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = versionSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `agent-version-${key}`,
    op = "agent.version.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const current = await write
    .prepare(
      "SELECT COALESCE(max(v.version_number),0)::int n FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE v.agent_id=? AND a.company_id=?",
    )
    .bind(i.agentId, a.companyId)
    .first<{ n: number }>();
  if (current === null)
    throw new ApiError(404, "AGENT_NOT_FOUND", "Agent not found.");
  const now = Date.now(),
    number = Number(current.n) + 1,
    result = { id, versionNumber: number, status: "draft", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    assertion(
      write,
      key,
      "agent",
      "EXISTS(SELECT 1 FROM agent_catalog WHERE id=? AND company_id=? AND status='approved')",
      [i.agentId, a.companyId],
    ),
    write
      .prepare(
        'INSERT INTO agent_versions VALUES(?,?,?,?,?,\'["approve","sign","pay"]\',\'draft\',?,NULL,1,?,?)',
      )
      .bind(
        id,
        i.agentId,
        number,
        i.prompt,
        JSON.stringify(i.allowedSources),
        a.userId,
        now,
        now,
      ),
  ]);
}
export async function createAgentPool(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = poolSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `agent-pool-${key}`,
    op = "agent.pool.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = { id, status: "active", version: 1 };
  const statements: SqlStatement[] = [
    write
      .prepare("INSERT INTO agent_pools VALUES(?,?,?,'active',?,1,?,?)")
      .bind(id, a.companyId, i.name, a.userId, now, now),
  ];
  for (const userId of [...new Set(i.userIds)])
    statements.push(
      assertion(write,key,`member-${statements.length}`,"EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?))",[a.companyId,userId,now,now]),
      write
        .prepare("INSERT INTO agent_pool_members(id,pool_id,user_id,created_by,created_at) VALUES(?,?,?,?,?)")
        .bind(
          `agent-pool-member-${key}-${await sha256(userId)}`,
          id,
          userId,
          a.userId,
          now,
        ),
    );
  statements.push(write.prepare("INSERT INTO agent_pool_revisions VALUES(?,?,1,?,?,?,?)").bind(`agent-pool-revision-${key}`,id,JSON.stringify([...new Set(i.userIds)].sort()),"Initial eligible routing members",a.userId,now));
  return executeCommand(write, a, key, op, i, id, 1, result, statements);
}
export async function changeAgentPool(a:WorkContext,id:string,raw:unknown,key:string) {
  access(a,true);
  const i=z.discriminatedUnion("action",[
    z.object({action:z.literal("replace-members"),expectedVersion:version,userIds:z.array(z.string().min(1)).min(1).max(100),reason}),
    z.object({action:z.literal("retire"),expectedVersion:version,reason}),
  ]).parse(raw),{write}=await holdingResources(a.holdingId),op=`agent.pool.${i.action}`,prior=await replayCommand(write,a,key,op,id,i);
  if(prior)return prior;
  const now=Date.now(),members=i.action==="replace-members"?[...new Set(i.userIds)].sort():[],steps:SqlStatement[]=[
    write.prepare("SELECT id FROM agent_pools WHERE id=? AND company_id=? FOR UPDATE").bind(id,a.companyId),
    assertion(write,key,"pool","EXISTS(SELECT 1 FROM agent_pools WHERE id=? AND company_id=? AND version=? AND status='active')",[id,a.companyId,i.expectedVersion]),
  ];
  if(i.action==="replace-members") {
    for(const userId of members)steps.push(assertion(write,key,`member-${steps.length}`,"EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?))",[a.companyId,userId,now,now]));
    steps.push(write.prepare("UPDATE agent_pool_members SET revoked_at=?,version=version+1 WHERE pool_id=? AND revoked_at IS NULL AND user_id NOT IN (SELECT jsonb_array_elements_text(?::jsonb))").bind(now,id,JSON.stringify(members)));
    for(const userId of members)steps.push(write.prepare("INSERT INTO agent_pool_members(id,pool_id,user_id,created_by,created_at) VALUES(?,?,?,?,?) ON CONFLICT(pool_id,user_id) DO UPDATE SET revoked_at=NULL,version=agent_pool_members.version+1 WHERE agent_pool_members.revoked_at IS NOT NULL")
      .bind(`agent-pool-member-${key}-${await sha256(userId)}`,id,userId,a.userId,now));
    steps.push(write.prepare("INSERT INTO agent_pool_revisions VALUES(?,?,?,?,?,?,?)").bind(`agent-pool-revision-${key}`,id,i.expectedVersion+1,JSON.stringify(members),i.reason,a.userId,now));
  }
  const status=i.action==="retire"?"retired":"active";
  steps.push(write.prepare("UPDATE agent_pools SET status=?,version=version+1,updated_at=? WHERE id=?").bind(status,now,id));
  return executeCommand(write,a,key,op,i,id,i.expectedVersion+1,{id,status,version:i.expectedVersion+1},steps);
}
export async function changePrivateSession(a:WorkContext,id:string,raw:unknown,key:string) {
  const i=z.object({action:z.enum(["hold","release-hold","expire"]),expectedVersion:version,reason}).parse(raw);
  requirePermission(a,i.action==="expire"?"records.read":"ai.manage");
  const {write}=await holdingResources(a.holdingId),op=`agent.session.${i.action}`,prior=await replayCommand(write,a,key,op,id,i);
  if(prior)return prior;
  const now=Date.now(),manage=hasPermission(a,"ai.manage"),steps:SqlStatement[]=[
    write.prepare("SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR UPDATE").bind(id),
    assertion(write,key,"private-session-control","EXISTS(SELECT 1 FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.id=? AND s.company_id=? AND (s.user_id=? OR ?=1) AND c.version=?)",[id,a.companyId,a.userId,manage?1:0,i.expectedVersion]),
  ];
  if(i.action==="expire")steps.push(assertion(write,key,"active-session","EXISTS(SELECT 1 FROM ai_private_session_controls WHERE session_id=? AND status='active')",[id]),write.prepare("UPDATE ai_private_session_controls SET status='expired',version=version+1,updated_at=? WHERE session_id=?").bind(now,id));
  else steps.push(assertion(write,key,"session-hold","EXISTS(SELECT 1 FROM ai_private_session_controls WHERE session_id=? AND legal_hold=?)",[id,i.action==="hold"?0:1]),write.prepare("UPDATE ai_private_session_controls SET legal_hold=?,hold_reason=?,version=version+1,updated_at=? WHERE session_id=?").bind(i.action==="hold"?1:0,i.action==="hold"?i.reason:null,now,id));
  steps.push(write.prepare("INSERT INTO ai_private_session_events VALUES(?,?,?,?,?,?,?)").bind(`ai-private-session-event-${key}`,id,i.expectedVersion+1,i.action,a.userId,i.reason,now));
  return executeCommand(write,a,key,op,i,id,i.expectedVersion+1,{id,version:i.expectedVersion+1,action:i.action},steps);
}
export async function createAgentAssignment(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = assignSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `agent-assignment-${key}`,
    op = "agent.assignment.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = { id, status: "active", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    assertion(
      write,
      key,
      "version",
      "EXISTS(SELECT 1 FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE v.id=? AND v.status='approved' AND a.status='approved' AND a.company_id=?)",
      [i.agentVersionId, a.companyId],
    ),
    ...(i.poolId
      ? [
          assertion(
            write,
            key,
            "pool",
            "EXISTS(SELECT 1 FROM agent_pools WHERE id=? AND company_id=? AND status='active')",
            [i.poolId, a.companyId],
          ),
        ]
      : [
          assertion(
            write,
            key,
            "user",
            "EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?))",
            [a.companyId, i.userId!, now, now],
          ),
        ]),
    write
      .prepare(
        "INSERT INTO agent_assignments VALUES(?,?,?,?,?,?,'active',?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.agentVersionId,
        i.poolId ?? null,
        i.userId ?? null,
        i.scopePath,
        a.userId,
        now,
        now,
      ),
  ]);
}
export async function changeAgentRecord(
  a: WorkContext,
  kind: "agent" | "version" | "assignment",
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = actionSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    op = `agent.${kind}.${i.action}`,
    prior = await replayCommand(write, a, key, op, id, i);
  if(prior)return prior;
  if(kind!=="agent"&&kind!=="version"&&kind!=="assignment" || kind==="assignment" && i.action!=="revoke" || kind!=="assignment" && i.action==="revoke")
    throw new ApiError(422,"AGENT_ACTION_INVALID","Choose an available lifecycle action for this record.");
  const
    table =
      kind === "agent"
        ? "agent_catalog"
        : kind === "version"
          ? "agent_versions"
          : "agent_assignments",
    row = await write
      .prepare(
        `SELECT x.* FROM ${table} x ${kind === "version" ? "JOIN agent_catalog a ON a.id=x.agent_id" : ""} WHERE x.id=? AND ${kind === "version" ? "a" : "x"}.company_id=?`,
      )
      .bind(id, a.companyId)
      .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "AGENT_RECORD_NOT_FOUND",
      "Agent record not found.",
    );
  const expected =
      i.action === "revoke"
        ? "active"
        : i.action === "retire"
          ? "approved"
          : "draft",
    status =
      i.action === "approve"
        ? "approved"
        : i.action === "reject"
          ? "rejected"
          : i.action === "retire"
            ? "retired"
            : "revoked";
  if (row.status !== expected || Number(row.version) !== i.expectedVersion)
    throw new ApiError(
      409,
      "CONFLICT",
      "Record changed or transition is invalid.",
    );
  if (row.created_by === a.userId && ["approve", "reject"].includes(i.action))
    throw new ApiError(409, "CONFLICT", "Independent review is required.");
  const now = Date.now(),
    review = kind === "assignment" ? "revoked_by" : "reviewed_by";
  return executeCommand(
    write,
    a,
    key,
    op,
    i,
    id,
    i.expectedVersion + 1,
    { id, status, version: i.expectedVersion + 1 },
    [
      assertion(
        write,
        key,
        "record",
        `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND status=?)`,
        [id, i.expectedVersion, expected],
      ),
      write
        .prepare(
          `UPDATE ${table} SET status=?,${review}=?,version=version+1,updated_at=? WHERE id=?`,
        )
        .bind(status, a.userId, now, id),
    ],
  );
}
export async function assistantGovernanceContext(
  db: SqlDatabase,
  a: WorkContext,
  conversationKey: string,
  scopeKey: string,
  path: string,
):Promise<AssistantGovernance> {
  const policy=await assistantPolicy(db,a,path),now=Date.now();
  const existing=await db.prepare("SELECT s.id,s.generation,c.status,c.expires_at FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=? AND s.user_id=? AND s.assignment_id=? AND s.scope_key=? AND s.conversation_key=? ORDER BY s.generation DESC LIMIT 1")
    .bind(a.companyId,a.userId,a.assignmentId,scopeKey,conversationKey).first<{id:string;generation:number;status:string;expires_at:number}>();
  if(existing?.status==="active"&&Number(existing.expires_at)>now)return {sessionId:existing.id,...policy};
  const generation=Number(existing?.generation??0)+1,id=`ai-session-${await sha256(JSON.stringify([a.companyId,a.userId,a.assignmentId,scopeKey,conversationKey,generation]))}`;
  await db.batch([
    db.prepare("INSERT INTO ai_private_sessions(id,company_id,user_id,assignment_id,scope_key,conversation_key,agent_assignment_id,created_at,last_used_at,generation) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,user_id,assignment_id,scope_key,conversation_key,generation) DO NOTHING")
      .bind(id,a.companyId,a.userId,a.assignmentId,scopeKey,conversationKey,policy.assignmentId,now,now,generation),
    db.prepare("INSERT INTO ai_private_session_controls(session_id,expires_at,last_used_at,updated_at) VALUES(?,?,?,?) ON CONFLICT(session_id) DO NOTHING").bind(id,now+30*24*60*60*1000,now,now),
  ]);
  return {sessionId:id,...policy};
}
export async function recordAssistantInvocation(
  db: SqlDatabase,
  a: WorkContext,
  input: {
    sessionId: string;
    agentVersionId: string | null;
    assignmentId?: string | null;
    assignmentVersion?: number | null;
    agentVersionRevision?: number | null;
    catalogVersion?: number | null;
    poolId?: string | null;
    poolVersion?: number | null;
    prompt?: string | null;
    allowedSources?: string[];
    turnId: string;
    question: string;
    sources: unknown;
    provider: unknown;
    outcome: "succeeded" | "failed";
    inputTokens?: number;
    outputTokens?: number;
    error?: string;
  },
) {
  await db
    .prepare(
      "INSERT INTO ai_invocation_evidence(id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,authority_snapshot_json,provider_snapshot_json,outcome,input_tokens,output_tokens,error,created_at,governance_snapshot_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(turn_id) DO NOTHING",
    )
    .bind(
      `ai-evidence-${input.turnId}`,
      input.sessionId,
      input.turnId,
      a.companyId,
      a.userId,
      input.agentVersionId,
      await sha256(input.question),
      await sha256(JSON.stringify(input.sources)),
      JSON.stringify({
        assignmentId: a.assignmentId,
        companyId: a.companyId,
        branchId: a.branchId,
        departmentId: a.departmentId,
      }),
      JSON.stringify(input.provider),
      input.outcome,
      input.inputTokens ?? null,
      input.outputTokens ?? null,
      input.error ?? null,
      Date.now(),
      JSON.stringify({assignmentId:input.assignmentId??null,assignmentVersion:input.assignmentVersion??null,agentVersionId:input.agentVersionId,agentVersionRevision:input.agentVersionRevision??null,catalogVersion:input.catalogVersion??null,poolId:input.poolId??null,poolVersion:input.poolVersion??null,promptDigest:input.prompt?await sha256(input.prompt):null,allowedSources:input.allowedSources??["/"]}),
    )
    .run();
}
