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
const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive();
const agentSchema = z.object({
  name: z.string().trim().min(2).max(120),
  purpose: reason,
});
const versionSchema = z.object({
  agentId: z.string().min(1),
  prompt: reason,
  allowedSources: z.array(z.string().min(1).max(160)).min(1).max(30),
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
    scopePath: z.string().startsWith("/").max(300),
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
export async function agentWorkspace(a: WorkContext) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    [agents, versions, pools, assignments, invocations, people] =
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
      ]);
  return {
    agents: agents.results,
    versions: versions.results,
    pools: pools.results,
    assignments: assignments.results,
    invocations: invocations.results,
    people: people.results,
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
      write
        .prepare("INSERT INTO agent_pool_members VALUES(?,?,?,?,?)")
        .bind(
          `agent-pool-member-${key}-${await sha256(userId)}`,
          id,
          userId,
          a.userId,
          now,
        ),
    );
  return executeCommand(write, a, key, op, i, id, 1, result, statements);
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
  const op = `agent.${kind}.${i.action}`,
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
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
) {
  const assignment = await db
      .prepare(
        "SELECT x.id,x.agent_version_id FROM agent_assignments x LEFT JOIN agent_pools p ON p.id=x.pool_id LEFT JOIN agent_pool_members m ON m.pool_id=p.id WHERE x.company_id=? AND x.status='active' AND ? LIKE x.scope_path||'%' AND (x.user_id=? OR m.user_id=?) ORDER BY length(x.scope_path) DESC,x.created_at DESC LIMIT 1",
      )
      .bind(a.companyId, path, a.userId, a.userId)
      .first<{ id: string; agent_version_id: string }>(),
    id = `ai-session-${await sha256(JSON.stringify([a.companyId, a.userId, a.assignmentId, scopeKey, conversationKey]))}`,
    now = Date.now();
  await db
    .prepare(
      "INSERT INTO ai_private_sessions(id,company_id,user_id,assignment_id,scope_key,conversation_key,agent_assignment_id,created_at,last_used_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,user_id,assignment_id,scope_key,conversation_key) DO NOTHING",
    )
    .bind(
      id,
      a.companyId,
      a.userId,
      a.assignmentId,
      scopeKey,
      conversationKey,
      assignment?.id ?? null,
      now,
      now,
    )
    .run();
  return {
    sessionId: id,
    agentVersionId: assignment?.agent_version_id ?? null,
  };
}
export async function recordAssistantInvocation(
  db: SqlDatabase,
  a: WorkContext,
  input: {
    sessionId: string;
    agentVersionId: string | null;
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
      "INSERT INTO ai_invocation_evidence VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(turn_id) DO NOTHING",
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
    )
    .run();
}
