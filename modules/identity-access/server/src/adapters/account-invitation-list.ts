import "server-only";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { createAccountInvitationListApplication, type AccountInvitationListRepository } from "@erpx/identity-access-server/application/account-invitation-list";
import type { AccountInvitationListItem, AccountInvitationListScope } from "@erpx/identity-access-server/domain/account-invitation-list";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";

/**
 * The activation panel read side is a native axum GET companion of the account
 * invitations command router (see `account_invitation_list_read` in the owner
 * crate). This adapter is a thin private transport: it forwards the exact
 * registered Holding Host/slug and the caller's Company-isolated scope to that
 * endpoint. No SQL, no fallback and no read path remain in TypeScript.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  timeoutMs: number;
}>;

function nativeConfig(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_ACCOUNT_INVITATION_LIST_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, "INVITATION_LIST_UNAVAILABLE", "The native invitation list service is not configured.");
  let value: { privateBaseUrl?: unknown; host?: unknown; holdingSlug?: unknown; origin?: unknown; timeoutMs?: unknown };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    throw new ApiError(503, "INVITATION_LIST_CONFIGURATION_INVALID", "The native invitation list transport is misconfigured.");
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new ApiError(503, "INVITATION_LIST_CONFIGURATION_INVALID", "The native invitation list transport is misconfigured.");
  }
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    origin.protocol !== "https:" ||
    typeof value.host !== "string" || value.host !== origin.host ||
    typeof value.holdingSlug !== "string" || !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30_000
  )
    throw new ApiError(503, "INVITATION_LIST_CONFIGURATION_INVALID", "The native invitation list transport is misconfigured.");
  return { base: base.origin, host: value.host, holdingSlug: value.holdingSlug, timeoutMs: value.timeoutMs as number };
}

/** Test seam for the panel read oracle. Production always uses the native reader. */
let testList: ((scope: AccountInvitationListScope) => Promise<AccountInvitationListItem[]>) | undefined;
export function setInvitationListForTest(list?: (scope: AccountInvitationListScope) => Promise<AccountInvitationListItem[]>) { testList = list; }

function integer(value: unknown): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) throw new ApiError(503, "INVITATION_LIST_UNAVAILABLE", "The native invitation list reply was invalid.");
  return value as number;
}
function nullableInteger(value: unknown): number | null {
  return value === null ? null : integer(value);
}
function nullableString(value: unknown): string | null {
  if (value === null) return null;
  if (typeof value !== "string") throw new ApiError(503, "INVITATION_LIST_UNAVAILABLE", "The native invitation list reply was invalid.");
  return value;
}
function requiredString(value: unknown): string {
  if (typeof value !== "string") throw new ApiError(503, "INVITATION_LIST_UNAVAILABLE", "The native invitation list reply was invalid.");
  return value;
}
function parseItem(value: unknown): AccountInvitationListItem {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new ApiError(503, "INVITATION_LIST_UNAVAILABLE", "The native invitation list reply was invalid.");
  const row = value as Record<string, unknown>;
  return {
    id: requiredString(row.id),
    person_id: requiredString(row.person_id),
    email: requiredString(row.email),
    status: requiredString(row.status),
    expires_at: integer(row.expires_at),
    delivery_state: nullableString(row.delivery_state),
    attempts: nullableInteger(row.attempts),
    error: nullableString(row.error),
    sent_at: nullableInteger(row.sent_at),
  };
}

/** Native-only repository. The axum read router owns the SQL and projection. */
function nativeRepository(): AccountInvitationListRepository {
  return {
    async list(scope) {
      if (testList) return testList(scope);
      const config = nativeConfig();
      const headers = new Headers({
        host: config.host,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-holding": scope.holdingId,
        "x-erpx-context-company": scope.companyId,
      });
      if (scope.branchId) headers.set("x-erpx-context-branch", scope.branchId);
      let response: Response;
      try {
        response = await fetch(
          `${config.base}/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/account/invitations`,
          { method: "GET", headers, cache: "no-store", signal: AbortSignal.timeout(config.timeoutMs) },
        );
      } catch {
        throw new ApiError(503, "INVITATION_LIST_UNAVAILABLE", "The native invitation list service is unavailable.");
      }
      if (!response.ok) {
        const value = await response.json().catch(() => null) as { error?: { code?: string; message?: string } } | null;
        throw new ApiError(
          response.status,
          value?.error?.code ?? "INVITATION_LIST_FAILED",
          value?.error?.message ?? "The invitation list could not be read.",
        );
      }
      const value = await response.json().catch(() => null) as { invitations?: unknown } | null;
      if (!value || !Array.isArray(value.invitations)) throw new ApiError(503, "INVITATION_LIST_UNAVAILABLE", "The native invitation list reply was invalid.");
      return value.invitations.map(parseItem);
    },
  };
}

/** Retained for callers that inject a database; the read itself is native-only. */
export function accountInvitationListRepository(): AccountInvitationListRepository {
  return nativeRepository();
}

export async function accountInvitationList(actor: WorkContext) {
  return createAccountInvitationListApplication(nativeRepository()).read(actor);
}
