import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { BusinessRuleError } from "@erpx/server-shared/index";
import {
  BRAND_RD_MANAGEMENT_BRANCH_EXISTS,
  BRAND_RD_MANAGEMENT_COMPANY_EXISTS,
  planBrandRead,
} from "./native-brand-read-ports";

/**
 * Strict native transport for the Brand management authority guards. The owner
 * plans both statements; this module emits no statement text and accepts none
 * from the wire. It executes the owner plan on the caller's already-open unit
 * and fails closed on every configuration, transport or row-shape failure.
 */
type Actor = Pick<WorkContext, "holdingId" | "holdingSlug" | "companyId" | "assignmentId" | "scope">;
type Branch = Readonly<{ id: string; display_name: string }>;

const unavailable = (code: string, status = 503): never => {
  throw new BusinessRuleError(status, code, "The native Brand management read is unavailable.");
};
const identifier = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= 200 && !/[\u0000-\u001f\u007f/\\]/.test(value);
const translate = (error: unknown): never => {
  if (error instanceof BusinessRuleError) throw error;
  if (error instanceof ApiError) throw new BusinessRuleError(error.status, error.code, error.message);
  return unavailable("NATIVE_BRAND_MANAGEMENT_READ_UNAVAILABLE");
};

/** Native active-Branch guard row for a management target. */
export async function nativeBrandManagementBranch(
  db: SqlDatabase,
  actor: Actor,
  id: string,
): Promise<Branch | null> {
  try {
    if (!identifier(id)) return unavailable("BRAND_MANAGEMENT_REQUEST_INVALID", 422);
    const plan = await planBrandRead(BRAND_RD_MANAGEMENT_BRANCH_EXISTS, [id, actor.companyId], {}, actor);
    const row = await db.prepare(plan.sql).bind(...plan.values).first<Record<string, unknown>>();
    if (row === null) return null;
    if (
      !identifier(row.id) ||
      typeof row.display_name !== "string" ||
      row.display_name.length === 0 ||
      new TextEncoder().encode(row.display_name).length > 2048
    )
      return unavailable("NATIVE_BRAND_MANAGEMENT_RESPONSE_INVALID", 502);
    return { id: row.id, display_name: row.display_name };
  } catch (error) {
    return translate(error);
  }
}

/** Native active-Company existence guard for a management target. */
export async function nativeBrandManagementCompanyExists(
  db: SqlDatabase,
  actor: Actor,
  id: string,
): Promise<boolean> {
  try {
    if (!identifier(id)) return unavailable("BRAND_MANAGEMENT_REQUEST_INVALID", 422);
    const plan = await planBrandRead(BRAND_RD_MANAGEMENT_COMPANY_EXISTS, [id], {}, actor);
    return (await db.prepare(plan.sql).bind(...plan.values).first()) !== null;
  } catch (error) {
    return translate(error);
  }
}
