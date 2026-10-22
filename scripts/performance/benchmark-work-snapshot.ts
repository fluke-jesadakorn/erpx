// Direct service benchmark against the current development selection. This is
// not an HTTP/authentication or compiler benchmark. Never use a staging manifest.

import { isDeepStrictEqual } from "node:util";
import { measureReads } from "@erpx/server-runtime/adapters/runtime/read-metrics";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { holdingResources } from "../../packages/server-runtime/src/adapters/runtime";
import { closeRuntime } from "@erpx/server-runtime/adapters/runtime/env";
import { closeRedisCache } from "@erpx/server-runtime/adapters/runtime/redis-cache";
import { loadIamAuthority } from "@erpx/server-identity_access/public/iam-store";
import { modulesForRole } from "@erpx/feature-identity_access/lib/business-roles";
import { readWorkSnapshot } from "@erpx/server-work_projection/public/work-items";
import { otherFlowWork } from "@erpx/server-workflow/public/flow-work";
import { withPolicyReadSnapshot } from "@erpx/server-identity_access/public/iam-read-snapshot";
import type { WorkAssignment, WorkContext } from "@erpx/server-identity_access/public/authorization";

if (process.env.NODE_ENV !== "development" || !["local", "development"].includes(process.env.ERPX_ENVIRONMENT ?? ""))
  throw new Error("DEVELOPMENT_BENCHMARK_ONLY");
const rounds = Number(process.env.ERPX_PERF_ROUNDS ?? 5);
if (!Number.isInteger(rounds) || rounds < 1 || rounds > 100) throw new Error("INVALID_ROUNDS");
process.env.ERPX_PERF_LOG = "0";
const holdingSlug = process.env.ERPX_PERF_HOLDING ?? "acme";
const holdingId = process.env.ERPX_PERF_HOLDING_ID ?? "h001";
const { write: db } = await holdingResources(holdingId);
// Guard the actual binding as well as the environment label.
const database = await db.prepare("SELECT current_database() name").first<{ name: string }>();
if (!database?.name.startsWith("erpx_dev_")) throw new Error("DEVELOPMENT_DATABASE_REQUIRED");
type Selection = Record<string, string | number>;
try {
  const now = Date.now();
  const s = await db.prepare(`SELECT c.*,a.role,a.scope_kind,a.scope_node_id,a.modules_json,a.valid_until,a.version
    FROM active_work_contexts c JOIN assignments a ON a.id=c.assignment_id
    WHERE c.expires_at>? AND a.scope_kind='holding' AND iam_assignment_active(a.id,?)
    ORDER BY c.updated_at DESC LIMIT 1`).bind(now, now).first<Selection>();
  if (!s) throw new Error("ACTIVE_DEVELOPMENT_SELECTION_REQUIRED");
  const iam = await loadIamAuthority(db, String(s.company_id), String(s.user_id), String(s.role), String(s.session_id));
  const assignment: WorkAssignment = { id: String(s.assignment_id), role: String(s.role), companyId: String(s.company_id),
    branchId: null, modules: JSON.parse(String(s.modules_json)), scopeKind: "holding", scopeNodeId: null,
    version: Number(s.version), validUntil: Number(s.valid_until) };
  const companyId = String(s.company_id);
  const actor: WorkContext = { iam, holdingId, holdingSlug, holdingName: "", companyId,
    userId: String(s.user_id), assignmentId: assignment.id, role: assignment.role, name: "", email: "",
    scope: { kind: "holding", holdingId, holdingSlug, holdingName: "", companyId },
    branchId: null, departmentId: null, assignmentScope: "holding", inherited: false,
    modules: modulesForRole(assignment.role, assignment.modules), assignments: [assignment], availableAssignments: [assignment],
    authority: { sessionId: String(s.session_id), revision: Number(s.revision), assignmentVersion: Number(s.assignment_version) },
    workspaces: [], beta: false };
  const queries = new Map<string, number>();
  const prepare = db.prepare.bind(db);
  db.prepare = (sql: string) => { queries.set(sql, (queries.get(sql) ?? 0) + 1); return prepare(sql); };
  const measured = await measureReads(() => withPolicyReadSnapshot(() => readWorkSnapshot(actor)));
  const flow = await measureReads(() => withPolicyReadSnapshot(() => otherFlowWork(actor)));
  const output = resolve(process.env.ERPX_PERF_OUTPUT ?? ".scratch/dev/work-snapshot-current.json");
  if (!output.startsWith(resolve(".scratch/dev") + "/")) throw new Error("PRIVATE_BENCHMARK_OUTPUT_REQUIRED");
  await mkdir(resolve(".scratch/dev"), { recursive: true });
  const observations = [];
  for (let round=1;round<rounds;round++) observations.push((await measureReads(() => withPolicyReadSnapshot(() => readWorkSnapshot(actor)))).metrics);
  const referencePath = process.env.ERPX_PERF_REFERENCE;
  let comparison;
  if (referencePath) {
    if (!resolve(referencePath).startsWith(resolve(".scratch/dev") + "/")) throw new Error("PRIVATE_REFERENCE_REQUIRED");
    const reference = JSON.parse(await readFile(referencePath,"utf8"));
    const normalize = (snapshot: typeof measured.result) => ({...snapshot, items: [...snapshot.items].sort((a,b)=>`${a.type}:${a.id}`.localeCompare(`${b.type}:${b.id}`))});
    if (reference.actor.companyId!==actor.companyId || reference.actor.userId!==actor.userId || reference.actor.assignmentId!==actor.assignmentId || reference.actor.authority.revision!==actor.authority?.revision) throw new Error("BENCHMARK_CONTEXT_CHANGED");
    if (!isDeepStrictEqual(normalize(reference.snapshot.result),normalize(JSON.parse(JSON.stringify(measured.result))))) throw new Error("BENCHMARK_DATA_CHANGED");
    comparison = {baselineQueries:reference.snapshot.metrics.queries, queries:measured.metrics.queries, reduction:1-measured.metrics.queries/reference.snapshot.metrics.queries, identical:true};
  }
  await writeFile(output, JSON.stringify({runtime:process.version,date: new Date().toISOString(), methodology:"Fresh read-only policy snapshot per round; durable/local Work cache bypassed. SQL and pool wait are cumulative concurrent work; duration excludes compiler, HTTP and browser. Compare only identical context and complete output.", actor, observations, comparison, queries: [...queries].sort((a,b)=>b[1]-a[1]), snapshot: measured, flow}, null, 2), {mode: 0o600});
  console.log(JSON.stringify({output, snapshot: measured.metrics, flow: flow.metrics, comparison, items: measured.result.items.length, unavailable: measured.result.unavailableSources}));
} finally { await closeRedisCache(); await closeRuntime(); }
