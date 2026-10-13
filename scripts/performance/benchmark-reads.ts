// Direct service benchmark against the current development selection. This is
// not an HTTP/authentication or compiler benchmark. Never use a staging manifest.
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { holdingResources } from "../../src/server/modules/runtime/adapters/runtime";
import { closeRuntime } from "../../src/server/modules/runtime/adapters/runtime/env";
import { closeRedisCache } from "../../src/server/modules/runtime/adapters/runtime/redis-cache";
import { measureReads, type ReadMeasurement } from "../../src/server/modules/runtime/adapters/runtime/read-metrics";
import { loadIamAuthority } from "../../src/server/modules/identity_access/public/iam-store";
import { modulesForRole } from "../../src/lib/identity_access/business-roles";
import { getWorkPage } from "../../src/server/modules/work_projection/public/work-summary";
import { holdingPeopleDirectory } from "../../src/server/modules/organization/public/holding-people-directory";
import { pmsWorkspace } from "../../src/server/modules/pms/public/pms";
import { pmsDashboardRecordsSql } from "../../src/server/modules/pms/public/pms/read-repository";
import type { WorkAssignment, WorkContext } from "../../src/server/modules/identity_access/public/authorization";

if (process.env.NODE_ENV !== "development" || !["local", "development"].includes(process.env.ERPX_ENVIRONMENT ?? ""))
  throw new Error("DEVELOPMENT_BENCHMARK_ONLY");
const rounds = Number(process.env.ERPX_PERF_ROUNDS ?? 20);
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
  const samples: Array<{ service: string; round: number; bytes?: number; metrics?: ReadMeasurement; failed: boolean }> = [];
  const loaders: Array<[string, () => Promise<unknown>]> = [
    ["work", () => getWorkPage(actor, { view: "actions", queue: "all", type: "all", q: "", responsibility: "all" })], ["people", () => holdingPeopleDirectory(actor)],
    ["pms", () => pmsWorkspace(actor, undefined, false, true)],
    ["pms_dashboard", () => pmsWorkspace(actor, undefined, false, true, { view: "dashboard" })],
  ];
  for (const [service, load] of loaders) {
    for (let round = 0; round < rounds; round++) {
      let metrics: ReadMeasurement | undefined;
      try {
        const measured = await measureReads(load, value => { metrics = value; });
        const serialized = JSON.stringify(measured.result);
        samples.push({ service, round, bytes: Buffer.byteLength(serialized), metrics, failed: false });
      } catch {
        samples.push({ service, round, metrics, failed: true });
      }
    }
  }
  const pms = await db.prepare("SELECT id,business_date FROM pms_properties WHERE company_id=? ORDER BY name LIMIT 1")
    .bind(companyId).first<{ id: string; business_date: string }>();
  let dashboardPlan: unknown;
  if (pms) {
    const row = await db.prepare(`EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ${pmsDashboardRecordsSql}`)
      .bind(pms.id, pms.business_date, pms.business_date, pms.id, pms.business_date, pms.business_date)
      .first<{ "QUERY PLAN": string }>();
    const raw = row ? JSON.parse(row["QUERY PLAN"])[0] : undefined;
    function summarize(plan: Record<string, unknown>): unknown {
      return Object.fromEntries(Object.entries(plan).filter(([key]) => ["Node Type", "Index Name", "Actual Rows", "Actual Loops", "Actual Total Time", "Plan Rows", "Shared Hit Blocks", "Shared Read Blocks", "Rows Removed by Filter", "Plans"].includes(key))
        .map(([key, value]) => [key, key === "Plans" ? (value as Record<string, unknown>[]).map(summarize) : value]));
    }
    if (raw) dashboardPlan = { planningMs: raw["Planning Time"], executionMs: raw["Execution Time"], plan: summarize(raw.Plan) };
  }
  const output = resolve(process.env.ERPX_PERF_OUTPUT ?? ".scratch/dev/backend-reads.json");
  if (!output.startsWith(resolve(".scratch/dev") + "/")) throw new Error("PRIVATE_BENCHMARK_OUTPUT_REQUIRED");
  await mkdir(resolve(".scratch/dev"), { recursive: true });
  await writeFile(output, JSON.stringify({ runtime: process.version, date: new Date().toISOString(),
    methodology: "Direct services, live development selection with fresh IAM. Excludes HTTP authentication, compilation and browser/network. Round zero is first service read in this process, not cold compilation. Contexts are selected afresh each run; compare live runs only after controlling actor, company and data revisions.", samples, dashboardPlan }, null, 2), { mode: 0o600 });
  console.log(JSON.stringify({ output, samples: samples.length, failures: samples.filter(s => s.failed).length }));
} finally { await closeRedisCache(); await closeRuntime(); }
