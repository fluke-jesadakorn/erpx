import "server-only";
import { Agent, request as httpRequest } from "node:http";
import type { IncomingMessage } from "node:http";
import { Readable } from "node:stream";
import { headers } from "next/headers";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {
  ProjectCostAllocationRow,
  ProjectWorkLogRow,
} from "@erpx/server-projects/domain/actuals";
import type { ProjectRow as NativeProjectRow } from "@erpx/server-projects/domain/staff-projects";

/**
 * Strict private transport for the lane 27 native Projects actuals reads.
 * The axum owner executes the SQL, owns the company/member scope, projects the
 * rows and computes `amount`; this bridge carries only identities, emits no
 * SQL and has no TypeScript fallback. Every registered Holding is supported:
 * the target is selected from the caller's WorkContext, never defaulted. Missing
 * configuration, an unknown target, a failed native route or an off-shape reply
 * fail closed with ApiError(503, ...).
 */
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingSlug: string;
  holdingId: string;
  companyId?: string;
}>;
type Config = Readonly<{ base: URL; holdings: readonly Holding[]; timeoutMs: number }>;
type Reply = Readonly<{ status: number; contentType: string | null; text: string }>;
export type ActualsTransport = (
  url: URL,
  outgoing: Readonly<Record<string, string>>,
  timeoutMs: number,
) => Promise<Reply>;

export type ProjectActualsActor = Pick<
  WorkContext,
  "companyId" | "userId" | "holdingId" | "holdingSlug"
>;

export type NativeHandoffRow = {
  id: string;
  company_id: string;
  project_id: string;
  source_project_version: number;
  recipient_id: string;
  status: string;
  version: number;
  snapshot_json: string;
  snapshot_sha256: string;
  evidence: string;
  prepared_by: string;
  prepared_at: number;
  decided_by: string | null;
  decided_at: number | null;
  decision_reason: string | null;
};

export type NativeHandoffWorkRow = {
  id: string;
  version: number;
  project_id: string;
  recipient_id: string;
  prepared_by: string;
  status: string;
  prepared_at: number;
  decided_at: number | null;
  title: string;
  lifecycle_status: string;
};

export type NativeHandoffProject = { project_id: string };

type ReadKind =
  | "work-logs"
  | "cost-allocations"
  | "handoffs"
  | "handoff-work"
  | "handoff-project"
  | "visible-project"
  | "visible-projects";
const ENVELOPES: Record<ReadKind, string> = {
  "work-logs": "workLogs",
  "cost-allocations": "costAllocations",
  handoffs: "handoffs",
  "handoff-work": "handoffs",
  "handoff-project": "handoff",
  "visible-project": "project",
  "visible-projects": "projects",
};
const ROUTES: Record<ReadKind, string> = {
  "work-logs": "projects/actuals/work-logs",
  "cost-allocations": "projects/actuals/cost-allocations",
  handoffs: "projects/handoffs",
  "handoff-work": "projects/handoff-work",
  "handoff-project": "projects/handoff-project",
  "visible-project": "projects/visible-project",
  "visible-projects": "projects/visible-projects",
};
const HANDOFF_STATUSES = ["pending", "accepted", "rejected", "cancelled"];
const LIFECYCLE_STATUSES = ["active", "handoff_pending", "completed", "cancelled"];

const ENV = "ERPX_NATIVE_PROJECTS_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_BODY = 4 * 1024 * 1024;

const unavailable = (code = "PROJECT_ACTUALS_LANE27_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native Projects actuals service is unavailable.");
};
const record = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const exact = (v: unknown, keys: readonly string[]): v is Record<string, unknown> =>
  record(v) && Object.keys(v).length === keys.length && keys.every((key) => Object.hasOwn(v, key));
const text = (v: unknown, max = 4096): v is string =>
  typeof v === "string" && new TextEncoder().encode(v).length <= max;
const identifier = (v: unknown): v is string =>
  text(v, 200) && v.length > 0 && /^[A-Za-z0-9_.:-]+$/.test(v);
const integer = (v: unknown, min = 0): v is number =>
  typeof v === "number" && Number.isSafeInteger(v) && v >= min;
const nullableText = (v: unknown, max = 4096): v is string | null => v === null || text(v, max);

function configuration(raw: string | undefined): Config {
  if (raw === undefined || raw === "") return unavailable("PROJECT_ACTUALS_LANE27_CONFIG_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("PROJECT_ACTUALS_LANE27_CONFIG_INVALID");
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl, 512) ||
    !integer(value.timeoutMs, 250) ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("PROJECT_ACTUALS_LANE27_CONFIG_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("PROJECT_ACTUALS_LANE27_CONFIG_INVALID");
  }
  const port = Number(base.port);
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return unavailable("PROJECT_ACTUALS_LANE27_CONFIG_INVALID");
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !identifier(item.holdingSlug) ||
      !identifier(item.holdingId)
    )
      return unavailable("PROJECT_ACTUALS_LANE27_CONFIG_INVALID");
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable("PROJECT_ACTUALS_LANE27_CONFIG_INVALID");
    }
    const hasCompany = Object.hasOwn(item, "companyId");
    if (hasCompany && !identifier(item.companyId))
      return unavailable("PROJECT_ACTUALS_LANE27_CONFIG_INVALID");
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      item.host !== origin.host ||
      holdings.some((holding) =>
        holding.origin === origin.origin ||
        holding.host === item.host ||
        (holding.holdingId === item.holdingId && holding.holdingSlug === item.holdingSlug),
      )
    )
      return unavailable("PROJECT_ACTUALS_LANE27_CONFIG_INVALID");
    holdings.push({
      origin: origin.origin,
      host: item.host as string,
      holdingSlug: item.holdingSlug as string,
      holdingId: item.holdingId as string,
      ...(hasCompany ? { companyId: item.companyId as string } : {}),
    });
  }
  return { base: new URL(base.origin), holdings, timeoutMs: value.timeoutMs as number };
}

/**
 * Choose exactly one registered Holding from the caller's WorkContext. There is
 * no first-holding default and no Host-only guess: an unknown or ambiguous
 * holding, a mismatched request origin or a mismatched configured Company fails
 * closed before any request reaches the native owner.
 */
function selectHolding(
  config: Config,
  actor: ProjectActualsActor,
  incoming: Pick<Headers, "get">,
): Holding {
  const matches = config.holdings.filter(
    (item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (matches.length !== 1) return unavailable("WORK_CONTEXT_CHANGED");
  const holding = matches[0]!;
  if (holding.companyId !== undefined && holding.companyId !== actor.companyId)
    return unavailable("WORK_CONTEXT_CHANGED");
  const origin = incoming.get("origin");
  if (!origin) return unavailable("PROJECT_ACTUALS_LANE27_REQUEST_INVALID");
  let originHost: string;
  try {
    originHost = new URL(origin).host;
  } catch {
    return unavailable("PROJECT_ACTUALS_LANE27_REQUEST_INVALID");
  }
  if (originHost !== new URL(holding.origin).host)
    return unavailable("WORK_CONTEXT_CHANGED");
  return holding;
}

/** Fresh loopback socket with an explicit Host header; fetch forbids overriding Host. */
const requestTransport: ActualsTransport = (url, outgoing, timeoutMs) =>
  new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import("node:http").ProxyEnv,
    });
    const req = httpRequest(
      url,
      { method: "GET", agent, headers: { ...outgoing }, signal: AbortSignal.timeout(timeoutMs), maxHeaderSize: 8192 },
      (incoming: IncomingMessage) => {
        incoming.once("close", () => agent.destroy());
        const chunks: Buffer[] = [];
        incoming.on("data", (chunk: Buffer) => chunks.push(chunk));
        incoming.on("end", () => {
          resolve({
            status: incoming.statusCode ?? 502,
            contentType: typeof incoming.headers["content-type"] === "string" ? incoming.headers["content-type"] : null,
            text: Buffer.concat(chunks).toString("utf8"),
          });
        });
        incoming.on("error", (error) => {
          agent.destroy();
          reject(error);
        });
      },
    );
    req.once("error", (error) => {
      agent.destroy();
      reject(error);
    });
    req.end();
  });

const WORK_LOG_KEYS = [
  "id", "project_id", "user_id", "work_date", "minutes", "state", "version",
  "reviewed_by", "rejection_reason", "created_at", "updated_at", "is_actor", "can_review",
] as const;
const COST_KEY_KEYS = [
  "id", "project_id", "journal_line_id", "amount_minor", "amount", "state", "version",
  "created_by", "approved_by", "rejection_reason", "created_at", "updated_at", "journal_id",
  "journal_number", "entry_date", "currency", "journal_status", "account_code", "account_name",
  "can_review", "is_creator",
] as const;

const invalidResponse = (): never =>
  unavailable("PROJECT_ACTUALS_LANE27_RESPONSE_INVALID");

function workLog(value: unknown): ProjectWorkLogRow {
  if (
    !exact(value, WORK_LOG_KEYS) ||
    !identifier(value.id) ||
    !identifier(value.project_id) ||
    !identifier(value.user_id) ||
    !text(value.work_date, 10) ||
    !/^\d{4}-\d{2}-\d{2}$/.test(value.work_date) ||
    !integer(value.minutes, 1) ||
    (value.minutes as number) > 1440 ||
    !["submitted", "approved", "rejected"].includes(String(value.state)) ||
    !integer(value.version, 1) ||
    !nullableText(value.reviewed_by, 200) ||
    !text(value.rejection_reason, 2000) ||
    !integer(value.created_at, 0) ||
    !integer(value.updated_at, 0) ||
    typeof value.is_actor !== "boolean" ||
    typeof value.can_review !== "boolean"
  )
    return invalidResponse();
  return value as unknown as ProjectWorkLogRow;
}

function costAllocation(value: unknown): ProjectCostAllocationRow {
  if (
    !exact(value, COST_KEY_KEYS) ||
    !identifier(value.id) ||
    !identifier(value.project_id) ||
    !identifier(value.journal_line_id) ||
    !text(value.amount_minor, 19) ||
    !/^-?\d+$/.test(value.amount_minor) ||
    !text(value.amount, 32) ||
    !/^-?\d+\.\d{2}$/.test(value.amount) ||
    !["draft", "approved", "rejected"].includes(String(value.state)) ||
    !integer(value.version, 1) ||
    !identifier(value.created_by) ||
    !nullableText(value.approved_by, 200) ||
    !text(value.rejection_reason, 2000) ||
    !integer(value.created_at, 0) ||
    !integer(value.updated_at, 0) ||
    !identifier(value.journal_id) ||
    !text(value.journal_number, 200) ||
    !text(value.entry_date, 10) ||
    !text(value.currency, 16) ||
    !text(value.journal_status, 40) ||
    !text(value.account_code, 200) ||
    !text(value.account_name, 500) ||
    typeof value.can_review !== "boolean" ||
    typeof value.is_creator !== "boolean"
  )
    return invalidResponse();
  return value as unknown as ProjectCostAllocationRow;
}

const HANDOFF_KEYS = [
  "id", "company_id", "project_id", "source_project_version", "recipient_id", "status", "version",
  "snapshot_json", "snapshot_sha256", "evidence", "prepared_by", "prepared_at", "decided_by",
  "decided_at", "decision_reason",
] as const;
const HANDOFF_WORK_KEYS = [
  "id", "version", "project_id", "recipient_id", "prepared_by", "status", "prepared_at",
  "decided_at", "title", "lifecycle_status",
] as const;
const PROJECT_KEYS = [
  "id", "company_id", "branch_id", "department_id", "title", "description",
  "owner_user_id", "lifecycle_status", "version", "updated_at",
] as const;

function handoffRow(value: unknown): NativeHandoffRow {
  if (
    !exact(value, HANDOFF_KEYS) ||
    !identifier(value.id) ||
    !identifier(value.company_id) ||
    !identifier(value.project_id) ||
    !identifier(value.recipient_id) ||
    !identifier(value.prepared_by) ||
    !integer(value.source_project_version, 1) ||
    !integer(value.version, 1) ||
    !integer(value.prepared_at, 0) ||
    !HANDOFF_STATUSES.includes(String(value.status)) ||
    !text(value.snapshot_json, 1_048_576) ||
    !text(value.snapshot_sha256, 64) ||
    !/^[a-f0-9]{64}$/.test(String(value.snapshot_sha256)) ||
    !text(value.evidence, 8192) ||
    !nullableText(value.decided_by, 200) ||
    (value.decided_at !== null && !integer(value.decided_at, 0)) ||
    !nullableText(value.decision_reason, 2000)
  )
    return invalidResponse();
  return value as unknown as NativeHandoffRow;
}

function handoffWorkRow(value: unknown): NativeHandoffWorkRow {
  if (
    !exact(value, HANDOFF_WORK_KEYS) ||
    !identifier(value.id) ||
    !identifier(value.project_id) ||
    !identifier(value.recipient_id) ||
    !identifier(value.prepared_by) ||
    !integer(value.version, 1) ||
    !integer(value.prepared_at, 0) ||
    !HANDOFF_STATUSES.includes(String(value.status)) ||
    !LIFECYCLE_STATUSES.includes(String(value.lifecycle_status)) ||
    !text(value.title, 200) ||
    (value.decided_at !== null && !integer(value.decided_at, 0))
  )
    return invalidResponse();
  return value as unknown as NativeHandoffWorkRow;
}

function handoffProject(value: unknown): NativeHandoffProject {
  if (!exact(value, ["project_id"]) || !identifier(value.project_id)) return invalidResponse();
  return value as unknown as NativeHandoffProject;
}

/** Exact mirror of the Rust `ProjectRow` projection; no extra schema column is
 * accepted and the owner-or-member scope is never re-evaluated in TypeScript. */
function visibleProjectRow(value: unknown): NativeProjectRow {
  if (
    !exact(value, PROJECT_KEYS) ||
    !identifier(value.id) ||
    !identifier(value.company_id) ||
    !nullableText(value.branch_id, 200) ||
    !nullableText(value.department_id, 200) ||
    !text(value.title, 200) ||
    value.title.length === 0 ||
    !text(value.description, 10_000) ||
    !identifier(value.owner_user_id) ||
    !LIFECYCLE_STATUSES.includes(String(value.lifecycle_status)) ||
    !integer(value.version, 1) ||
    !integer(value.updated_at, 0)
  )
    return invalidResponse();
  return value as unknown as NativeProjectRow;
}

async function read(
  config: Config,
  actor: ProjectActualsActor,
  holding: Holding,
  kind: ReadKind,
  ids: { projectId?: string; handoffId?: string },
  transport: ActualsTransport,
): Promise<unknown> {
  if (
    !identifier(actor.companyId) ||
    !identifier(actor.userId) ||
    !identifier(actor.holdingId) ||
    !identifier(actor.holdingSlug)
  )
    return unavailable("PROJECT_ACTUALS_LANE27_REQUEST_INVALID");
  const headers: Record<string, string> = {
    host: holding.host,
    origin: holding.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": actor.companyId,
    "x-erpx-context-user": actor.userId,
  };
  if (ids.projectId !== undefined) {
    if (!identifier(ids.projectId)) return unavailable("PROJECT_ACTUALS_LANE27_REQUEST_INVALID");
    headers["x-erpx-context-project"] = ids.projectId;
  }
  if (ids.handoffId !== undefined) {
    if (!identifier(ids.handoffId)) return unavailable("PROJECT_ACTUALS_LANE27_REQUEST_INVALID");
    headers["x-erpx-context-handoff"] = ids.handoffId;
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/${ROUTES[kind]}`,
    config.base,
  );
  let reply: Reply;
  try {
    reply = await transport(url, headers, config.timeoutMs);
  } catch {
    return unavailable();
  }
  if (reply.status !== 200) {
    let payload: unknown = null;
    try {
      payload = JSON.parse(reply.text);
    } catch {
      payload = null;
    }
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    if (reply.status >= 400 && reply.status < 500)
      throw new ApiError(
        reply.status,
        text(error?.code, 100) ? String(error!.code) : "PROJECT_ACTUALS_LANE27_READ_FAILED",
        text(error?.message, 300) ? String(error!.message) : "The native Projects actuals read was rejected.",
      );
    return unavailable();
  }
  if (
    (reply.contentType ?? "").split(";")[0]?.trim() !== "application/json" ||
    new TextEncoder().encode(reply.text).length > MAX_BODY
  )
    return invalidResponse();
  let value: unknown;
  try {
    value = JSON.parse(reply.text);
  } catch {
    return invalidResponse();
  }
  const envelope = ENVELOPES[kind];
  if (!exact(value, [envelope])) return invalidResponse();
  return value[envelope];
}

export interface ProjectActualsReader {
  workLogs(projectId: string): Promise<ProjectWorkLogRow[]>;
  costAllocations(projectId: string): Promise<ProjectCostAllocationRow[]>;
  handoffs(projectId: string): Promise<NativeHandoffRow[]>;
  handoffWork(): Promise<NativeHandoffWorkRow[]>;
  handoffProject(handoffId: string): Promise<NativeHandoffProject | null>;
  visibleProject(projectId: string): Promise<NativeProjectRow | null>;
  visibleProjects(): Promise<NativeProjectRow[]>;
}

const rowsFrom = (value: unknown): unknown[] => {
  if (!Array.isArray(value)) return invalidResponse();
  return value;
};

/**
 * Transport factory. The Holding is selected from the caller's WorkContext and
 * its request Origin is checked against that exact target; the native route
 * receives the registered Host and Origin. `transport` is injectable for tests.
 */
export function createProjectActualsReader(
  actor: ProjectActualsActor,
  incoming: Pick<Headers, "get">,
  environment: Readonly<Record<string, string | undefined>> = process.env,
  transport: ActualsTransport = requestTransport,
): ProjectActualsReader {
  const config = configuration(environment[ENV]);
  const holding = selectHolding(config, actor, incoming);
  return {
    async workLogs(projectId) {
      const value = await read(config, actor, holding, "work-logs", { projectId }, transport);
      return rowsFrom(value).map(workLog);
    },
    async costAllocations(projectId) {
      const value = await read(config, actor, holding, "cost-allocations", { projectId }, transport);
      return rowsFrom(value).map(costAllocation);
    },
    async handoffs(projectId) {
      const value = await read(config, actor, holding, "handoffs", { projectId }, transport);
      return rowsFrom(value).map(handoffRow);
    },
    async handoffWork() {
      const value = await read(config, actor, holding, "handoff-work", {}, transport);
      return rowsFrom(value).map(handoffWorkRow);
    },
    async handoffProject(handoffId) {
      const value = await read(config, actor, holding, "handoff-project", { handoffId }, transport);
      return value === null ? null : handoffProject(value);
    },
    async visibleProject(projectId) {
      const value = await read(config, actor, holding, "visible-project", { projectId }, transport);
      return value === null ? null : visibleProjectRow(value);
    },
    async visibleProjects() {
      const value = await read(config, actor, holding, "visible-projects", {}, transport);
      return rowsFrom(value).map(visibleProjectRow);
    },
  };
}

/** Reader bound to the current Next request headers. */
export async function nativeProjectActualsReader(
  actor: ProjectActualsActor,
): Promise<ProjectActualsReader> {
  return createProjectActualsReader(actor, await headers());
}

export async function nativeProjectWorkLogs(
  actor: ProjectActualsActor,
  projectId: string,
): Promise<ProjectWorkLogRow[]> {
  return (await nativeProjectActualsReader(actor)).workLogs(projectId);
}

export async function nativeProjectCostAllocations(
  actor: ProjectActualsActor,
  projectId: string,
): Promise<ProjectCostAllocationRow[]> {
  return (await nativeProjectActualsReader(actor)).costAllocations(projectId);
}

export async function nativeProjectHandoffs(
  actor: ProjectActualsActor,
  projectId: string,
): Promise<NativeHandoffRow[]> {
  return (await nativeProjectActualsReader(actor)).handoffs(projectId);
}

export async function nativeProjectHandoffWork(
  actor: ProjectActualsActor,
): Promise<NativeHandoffWorkRow[]> {
  return (await nativeProjectActualsReader(actor)).handoffWork();
}

export async function nativeProjectHandoffProject(
  actor: ProjectActualsActor,
  handoffId: string,
): Promise<NativeHandoffProject | null> {
  return (await nativeProjectActualsReader(actor)).handoffProject(handoffId);
}

export async function nativeProjectVisibleProject(
  actor: ProjectActualsActor,
  projectId: string,
): Promise<NativeProjectRow | null> {
  return (await nativeProjectActualsReader(actor)).visibleProject(projectId);
}

export async function nativeProjectVisibleProjects(
  actor: ProjectActualsActor,
): Promise<NativeProjectRow[]> {
  return (await nativeProjectActualsReader(actor)).visibleProjects();
}
