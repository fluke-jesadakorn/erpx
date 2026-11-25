import "server-only";
import { Agent, request as httpRequest } from "node:http";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Strict transport for the native manage_erpx beta feedback read. The axum
 * owner executes the only statement and returns the projected rows; this bridge
 * emits no SQL, accepts none from a caller and fails closed with a 503 when the
 * native owner is unavailable or its response is malformed. No fallback query
 * remains in TypeScript.
 */
export type BetaFeedbackRow = {
  id: string;
  body: string;
  created_at: number;
};

type Holding = Readonly<{ origin: string; holdingSlug: string }>;
type Settings = Readonly<{ base: string; timeoutMs: number; holding: Holding }>;
export type FeedbackPost = (
  url: URL,
  headers: Record<string, string>,
  body: string,
  timeoutMs: number,
) => Promise<{ status: number; value: unknown }>;

export const NATIVE_FEEDBACK_ENV = "ERPX_NATIVE_MANAGE_ERPX_FEEDBACK_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_FEEDBACK = 100;
const MAX_ID = 200;
const MAX_BODY = 3000;

const record = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const text = (v: unknown, max: number): v is string =>
  typeof v === "string" && new TextEncoder().encode(v).length <= max;

const unavailable = (
  code = "MANAGE_ERPX_NATIVE_FEEDBACK_UNAVAILABLE",
  status = 503,
  message = "The native Manage ERPX feedback read is unavailable.",
): never => {
  throw new ApiError(status, code, message);
};

/** Closed loopback/front-origin configuration. Any malformed value fails closed. */
export function feedbackConfig(raw: string | undefined): Settings {
  if (raw === undefined) return unavailable("MANAGE_ERPX_NATIVE_FEEDBACK_UNAVAILABLE");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== "number" ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length !== 1
  )
    return unavailable("MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
  }
  const port = Number(base.port);
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.origin !== value.privateBaseUrl ||
    base.username ||
    base.password
  )
    return unavailable("MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
  const holding = value.holdings[0];
  if (
    !record(holding) ||
    !text(holding.origin, 512) ||
    !text(holding.holdingSlug, 128) ||
    !/^[a-z0-9_-]+$/.test(String(holding.holdingSlug))
  )
    return unavailable("MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
  let origin: URL;
  try {
    origin = new URL(String(holding.origin));
  } catch {
    return unavailable("MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
  }
  if (origin.protocol !== "https:" || origin.origin !== holding.origin || origin.username || origin.password)
    return unavailable("MANAGE_ERPX_NATIVE_FEEDBACK_CONFIG_INVALID");
  return {
    base: base.origin,
    timeoutMs: value.timeoutMs,
    holding: {
      origin: origin.origin,
      holdingSlug: String(holding.holdingSlug),
    },
  };
}

/** Company ids are bounded binds only; never interpolated into SQL or a path. */
export function validFeedbackCompany(company: string): boolean {
  return /^[a-zA-Z0-9:_.-]{1,200}$/.test(company);
}

/** Strictly validate the owner response. Extra fields are rejected, not passed through. */
export function parseFeedbackResponse(value: unknown): BetaFeedbackRow[] {
  const invalid = () => unavailable("MANAGE_ERPX_NATIVE_FEEDBACK_RESPONSE_INVALID");
  if (!record(value) || !Array.isArray(value.feedback) || value.feedback.length > MAX_FEEDBACK)
    return invalid();
  return value.feedback.map((row) => {
    if (
      !record(row) ||
      Object.keys(row).length !== 3 ||
      typeof row.id !== "string" ||
      row.id.length === 0 ||
      row.id.length > MAX_ID ||
      typeof row.body !== "string" ||
      row.body.length > MAX_BODY ||
      typeof row.created_at !== "number" ||
      !Number.isSafeInteger(row.created_at) ||
      row.created_at < 0
    )
      return invalid();
    return { id: row.id, body: row.body, created_at: row.created_at };
  });
}

/** POST with an explicit Host header; fetch forbids overriding Host. */
function httpPost(
  url: URL,
  headers: Record<string, string>,
  body: string,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import("node:http").ProxyEnv });
    const outgoing = httpRequest(
      url,
      {
        method: "GET",
        agent,
        headers: { ...headers, "content-length": String(Buffer.byteLength(body)) },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      (incoming) => {
        const chunks: Buffer[] = [];
        incoming.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
        incoming.on("end", () => {
          agent.destroy();
          const raw = Buffer.concat(chunks).toString("utf8");
          let parsed: unknown = null;
          try {
            parsed = raw ? JSON.parse(raw) : null;
          } catch {
            parsed = null;
          }
          resolve({ status: incoming.statusCode ?? 502, value: parsed });
        });
        incoming.on("error", (error) => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.on("error", (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}

/** Ask the native owner for the whole feedback projection; no SQL comes back. */
export async function nativeBetaFeedback(
  company: string,
  post: FeedbackPost = httpPost,
): Promise<BetaFeedbackRow[]> {
  if (!validFeedbackCompany(company))
    throw new ApiError(
      422,
      "MANAGE_ERPX_NATIVE_FEEDBACK_REQUEST_INVALID",
      "Use a bounded Company id.",
    );
  const cfg = feedbackConfig(process.env[NATIVE_FEEDBACK_ENV]);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(cfg.holding.holdingSlug)}/manage-erpx/beta-feedback`,
    cfg.base,
  );
  let reply: { status: number; value: unknown };
  try {
    reply = await post(
      url,
      {
        host: new URL(cfg.holding.origin).host,
        origin: cfg.holding.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": company,
      },
      "",
      cfg.timeoutMs,
    );
  } catch {
    return unavailable();
  }
  if (reply.status !== 200) {
    const error = record(reply.value) && record(reply.value.error) ? reply.value.error : undefined;
    if (reply.status >= 400 && reply.status < 500)
      throw new ApiError(
        reply.status,
        text(error?.code, 100) ? String(error!.code) : "MANAGE_ERPX_NATIVE_FEEDBACK_FAILED",
        text(error?.message, 300) ? String(error!.message) : "The native feedback read was rejected.",
      );
    return unavailable();
  }
  return parseFeedbackResponse(reply.value);
}

/**
 * Multi-Holding transport for the native beta invitations read. The owner
 * executes the company-scoped join and computes `invitation_state`; this bridge
 * only selects the exact registered Holding target, transports the bounded
 * Company id and validates the returned rows. No SQL or state calculation is
 * performed here, and no fallback query exists.
 */
export type BetaInvitationRow = {
  id: string;
  email: string;
  branch_id: string;
  expires_at: number;
  accepted_at: number | null;
  revoked_at: number | null;
  invitation_state: string;
};

type MultiHolding = Readonly<{
  origin: string;
  host: string;
  holdingSlug: string;
}>;
type MultiSettings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: MultiHolding[];
}>;
const INVITATION_STATES = new Set(["Revoked", "Joined", "Expired", "Invited"]);
export const NATIVE_INVITATIONS_ENV = "ERPX_NATIVE_MANAGE_ERPX_INVITATIONS_CONFIG";

const unavailableInvitations = (
  code = "MANAGE_ERPX_NATIVE_INVITATIONS_UNAVAILABLE",
  status = 503,
  message = "The native Manage ERPX invitations read is unavailable.",
): never => {
  throw new ApiError(status, code, message);
};

/** Exact, distinct registered Holdings only; any drift fails closed. */
export function invitationsConfig(raw: string | undefined): MultiSettings {
  if (raw === undefined) return unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl, 512) ||
    !Number.isSafeInteger(value.timeoutMs) ||
    Number(value.timeoutMs) < 250 ||
    Number(value.timeoutMs) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
  }
  const port = Number(base.port);
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.origin !== value.privateBaseUrl ||
    base.username ||
    base.password
  )
    return unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
  const holdings: MultiHolding[] = [];
  const slugs = new Set<string>();
  const hosts = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(String(item.holdingSlug))
    )
      return unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.host !== item.host ||
      origin.username ||
      origin.password ||
      slugs.has(String(item.holdingSlug)) ||
      hosts.has(String(item.host))
    )
      return unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_CONFIGURATION_INVALID");
    slugs.add(String(item.holdingSlug));
    hosts.add(String(item.host));
    holdings.push({
      origin: origin.origin,
      host: origin.host,
      holdingSlug: String(item.holdingSlug),
    });
  }
  return { base: base.origin, timeoutMs: Number(value.timeoutMs), holdings };
}

/** Strict owner response validation; unknown states and extra fields fail closed. */
export function parseInvitationsResponse(value: unknown): BetaInvitationRow[] {
  const invalid = () => unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_RESPONSE_INVALID");
  if (!record(value) || !Array.isArray(value.invitations) || value.invitations.length > MAX_FEEDBACK)
    return invalid();
  return value.invitations.map((row) => {
    const stamp = (input: unknown): number | null => {
      if (input === null) return null;
      if (typeof input !== "number" || !Number.isSafeInteger(input) || input < 0) return invalid();
      return input;
    };
    if (
      !record(row) ||
      Object.keys(row).length !== 7 ||
      typeof row.id !== "string" ||
      row.id.length === 0 ||
      row.id.length > MAX_ID ||
      typeof row.email !== "string" ||
      row.email.length === 0 ||
      row.email.length > 320 ||
      typeof row.branch_id !== "string" ||
      row.branch_id.length === 0 ||
      row.branch_id.length > MAX_ID ||
      typeof row.expires_at !== "number" ||
      !Number.isSafeInteger(row.expires_at) ||
      row.expires_at < 0 ||
      typeof row.invitation_state !== "string" ||
      !INVITATION_STATES.has(row.invitation_state)
    )
      return invalid();
    const acceptedAt = stamp(row.accepted_at);
    const revokedAt = stamp(row.revoked_at);
    return {
      id: row.id,
      email: row.email,
      branch_id: row.branch_id,
      expires_at: row.expires_at,
      accepted_at: acceptedAt,
      revoked_at: revokedAt,
      invitation_state: row.invitation_state,
    };
  });
}

/** Read invitations from the exact Holding named by the caller; no SQL is emitted. */
export async function nativeBetaInvitations(
  company: string,
  holdingSlug: string,
  post: FeedbackPost = httpPost,
): Promise<BetaInvitationRow[]> {
  if (!validFeedbackCompany(company))
    throw new ApiError(
      422,
      "MANAGE_ERPX_NATIVE_INVITATIONS_REQUEST_INVALID",
      "Use a bounded Company id.",
    );
  if (!/^[a-z0-9_-]{1,128}$/.test(holdingSlug))
    return unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_TARGET_INVALID");
  const cfg = invitationsConfig(process.env[NATIVE_INVITATIONS_ENV]);
  const holding = cfg.holdings.find((item) => item.holdingSlug === holdingSlug);
  if (!holding)
    return unavailableInvitations("MANAGE_ERPX_NATIVE_INVITATIONS_TARGET_UNAVAILABLE");
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/manage-erpx/beta-invitations`,
    cfg.base,
  );
  let reply: { status: number; value: unknown };
  try {
    reply = await post(
      url,
      {
        host: holding.host,
        origin: holding.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": company,
      },
      "",
      cfg.timeoutMs,
    );
  } catch {
    return unavailableInvitations();
  }
  if (reply.status !== 200) {
    const error = record(reply.value) && record(reply.value.error) ? reply.value.error : undefined;
    if (reply.status >= 400 && reply.status < 500)
      throw new ApiError(
        reply.status,
        text(error?.code, 100) ? String(error!.code) : "MANAGE_ERPX_NATIVE_INVITATIONS_FAILED",
        text(error?.message, 300)
          ? String(error!.message)
          : "The native invitations read was rejected.",
      );
    return unavailableInvitations();
  }
  return parseInvitationsResponse(reply.value);
}
