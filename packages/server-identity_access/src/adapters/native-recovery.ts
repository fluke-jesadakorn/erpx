import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import { nativeAccountHttp } from "./native-account-http";

type Send = (request: Request) => Promise<Response>;
const MAXIMUM_BODY_BYTES = 256 * 1024;

export type EmergencyAccessSeed = {
  access: {
    companyId: string;
    holdingSlug: string | null;
    scopeLabel: string;
    reason: string;
    expiresAt: number;
  }[];
  unavailableScopes: string[];
};

function seed(value: unknown): EmergencyAccessSeed {
  if (!value || typeof value !== "object" || Array.isArray(value)) return invalid();
  const record = value as Record<string, unknown>;
  if (!Array.isArray(record.access) || !Array.isArray(record.unavailableScopes)) return invalid();
  const access = record.access.map(row => {
    if (!row || typeof row !== "object" || Array.isArray(row)) return invalid();
    const item = row as Record<string, unknown>;
    if (
      typeof item.companyId !== "string" ||
      (item.holdingSlug !== null && typeof item.holdingSlug !== "string") ||
      typeof item.scopeLabel !== "string" ||
      typeof item.reason !== "string" ||
      typeof item.expiresAt !== "number" ||
      !Number.isSafeInteger(item.expiresAt)
    ) {
      return invalid();
    }
    return {
      companyId: item.companyId,
      holdingSlug: item.holdingSlug,
      scopeLabel: item.scopeLabel,
      reason: item.reason,
      expiresAt: item.expiresAt,
    };
  });
  const unavailableScopes = record.unavailableScopes.map(scope => {
    if (typeof scope !== "string") return invalid();
    return scope;
  });
  return { access, unavailableScopes };
}

function invalid(message = "The native emergency access response did not match its contract."): never {
  throw new ApiError(503, "NATIVE_EMERGENCY_ACCESS_RESPONSE_INVALID", message);
}

function failure(status: number, body: unknown): never {
  const record =
    body && typeof body === "object" && !Array.isArray(body)
      ? (body as Record<string, unknown>)
      : undefined;
  const error =
    record?.error && typeof record.error === "object" && !Array.isArray(record.error)
      ? (record.error as Record<string, unknown>)
      : undefined;
  const code =
    typeof error?.code === "string" && /^[A-Z0-9_]{3,80}$/.test(error.code)
      ? error.code
      : "NATIVE_EMERGENCY_ACCESS_UNAVAILABLE";
  const message =
    typeof error?.message === "string" && error.message.length <= 1000
      ? error.message
      : "The emergency access request could not be completed.";
  throw new ApiError(status, code, message);
}

async function call(
  send: Send,
  incoming: Request,
  path: string,
  init: { method: "GET" | "POST"; body?: unknown; key?: string },
) {
  const host = incoming.headers.get("host");
  if (!host) {
    throw new ApiError(
      503,
      "NATIVE_EMERGENCY_ACCESS_AUTHORITY_REQUIRED",
      "The native Account authority is not configured.",
    );
  }
  const headers = new Headers({
    host,
    accept: "application/json",
    "cache-control": "no-cache",
  });
  const cookie = incoming.headers.get("cookie");
  if (cookie) headers.set("cookie", cookie);
  const origin = incoming.headers.get("origin");
  if (origin) headers.set("origin", origin);
  if (init.key) headers.set("idempotency-key", init.key);
  if (init.body !== undefined) headers.set("content-type", "application/json");
  const response = await send(
    new Request(`https://${host}${path}`, {
      method: init.method,
      headers,
      body: init.body === undefined ? undefined : JSON.stringify(init.body),
      cache: "no-store",
      redirect: "error",
    }),
  );
  const text = await response.text();
  if (new TextEncoder().encode(text).length > MAXIMUM_BODY_BYTES) return invalid();
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    return invalid();
  }
  if (!response.ok) return failure(response.status, body);
  const data =
    body && typeof body === "object" && !Array.isArray(body)
      ? (body as Record<string, unknown>).data
      : undefined;
  if (data === undefined) return invalid();
  return data;
}

/** Native emergency-access transport. Identity always comes from the presented
 * Account session; the body only selects scope and reason. */
export function createNativeEmergencyAccess(send: Send = nativeAccountHttp) {
  return {
    active: async (request: Request) =>
      seed(await call(send, request, "/api/v1/account/iam/emergency/active", { method: "GET" })),
    activate: async (request: Request, key: string, body: unknown) =>
      (await call(send, request, "/api/v1/account/iam/emergency/activate", {
        method: "POST",
        body,
        key,
      })) as { expiresAt?: number; message?: string },
    revoke: async (request: Request, key: string, body: unknown) =>
      (await call(send, request, "/api/v1/account/iam/emergency/revoke", {
        method: "POST",
        body,
        key,
      })) as { revoked?: boolean; message?: string },
  };
}

export const nativeEmergencyAccess = createNativeEmergencyAccess();
