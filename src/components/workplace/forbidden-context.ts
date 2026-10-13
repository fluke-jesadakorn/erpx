import "server-only";

import { headers } from "next/headers";
import { unstable_rethrow } from "next/navigation";
import { safeReturnPath } from "@/lib/shared/access-problem";
import { ApiError } from "@/server/modules/runtime/public/http";

export function accessUnavailable(error: unknown) {
  unstable_rethrow(error);
  if (error instanceof ApiError && ["EMERGENCY_ACCESS_REQUIRED", "FRESH_MFA_REQUIRED"].includes(error.code)) return false;
  if (error instanceof ApiError && [403, 404, 409].includes(error.status)) return true;
  return error instanceof Error && [
    "FORBIDDEN", "HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND",
    "COMPANY_REQUIRED", "INVALID_ORGANIZATION_LINEAGE",
  ].includes(error.message);
}

export async function requestedPage() {
  const path = (await headers()).get("x-erpx-page-path");
  if (!path) return null;
  const url = new URL(path, "https://erpx.invalid");
  return safeReturnPath(url.pathname === "/work-role" ? url.searchParams.get("from") : path);
}
