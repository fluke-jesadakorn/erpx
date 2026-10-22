import "server-only";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { PlatformContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { createPlatformExecutiveApplication, requirePlatformExecutiveAccess } from "@erpx/server-platform_administration/application/platform-executive";
import type { Group } from "@erpx/server-platform_administration/domain/platform-executive";
function application(actor: PlatformContext, control: SqlDatabase, controlRead: SqlDatabase) {
  return createPlatformExecutiveApplication({ canRead: () => hasPermission(actor, "platform.executive.read"), clock: { now: () => Date.now() }, hash: sha256, error: (status, code, message) => new ApiError(status, code, message), repository: {
    failed: async (fromAt, toAt) => (await control.prepare(`SELECT to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label,count(*)::text value FROM platform_holding_provisioning_events WHERE action='failed' AND created_at>=? AND created_at<? GROUP BY 1 ORDER BY 1`).bind(fromAt, toAt).all<Group>()).results,
    abnormal: async () => (await controlRead.prepare(`SELECT component||' · '||status label,count(*)::text value FROM platform_health WHERE lower(status) NOT IN ('healthy','ready','ok') GROUP BY component,status ORDER BY component,status`).all<Group>()).results,
    failedSources: async (fromAt, toAt, label, offset) => (await control.prepare(`SELECT created_at,action FROM platform_holding_provisioning_events WHERE action='failed' AND created_at>=? AND created_at<? AND to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')=? ORDER BY created_at,id LIMIT 31 OFFSET ?`).bind(fromAt, toAt, label, offset).all<{ created_at: number; action: string }>()).results,
    abnormalSources: async (label, offset) => (await controlRead.prepare(`SELECT component,status,checked_at FROM platform_health WHERE lower(status) NOT IN ('healthy','ready','ok') AND component||' · '||status=? ORDER BY component LIMIT 31 OFFSET ?`).bind(label, offset).all<{ component: string; status: string; checked_at: number }>()).results,
  } });
}
export function requirePlatformExecutive(actor: PlatformContext) { requirePlatformExecutiveAccess(hasPermission(actor, "platform.executive.read"), (status, code, message) => new ApiError(status, code, message)); return actor; }
export function platformExecutiveSnapshot(actor: PlatformContext, control: SqlDatabase, controlRead: SqlDatabase, rawFilters?: unknown) { return application(actor, control, controlRead).platformExecutiveSnapshot(rawFilters); }
export function platformExecutiveSources(actor: PlatformContext, control: SqlDatabase, controlRead: SqlDatabase, raw: unknown) { return application(actor, control, controlRead).platformExecutiveSources(raw); }
