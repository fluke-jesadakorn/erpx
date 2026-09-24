import "server-only";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import type { PlatformContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { sha256 } from "./crypto";

const dateFilters = z.object({
  from: z.iso.date(),
  to: z.iso.date(),
}).strict().refine(({ from, to }) => from <= to && Date.parse(to) - Date.parse(from) <= 366 * 86_400_000, "Choose a date range of up to one year.");

type Group = { label: string; value: string };

export type PlatformExecutiveSnapshot = {
  scope: "platform";
  referenceAt: number;
  filters: { from: string; to: string };
  sourceRevision: string;
  metrics: {
    id: "failed-provisioning" | "abnormal-platform-services";
    label: string;
    formula: string;
    dateBasis: string;
    total: string;
    rows: Group[];
    status: "ready" | "empty";
    sourceReferences: { dataset: string; revision: string; referencedAt: number }[];
  }[];
};

const exact = (value: string | number | bigint) => {
  if (typeof value === "bigint") return value;
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value)) throw new ApiError(500, "PLATFORM_METRIC_NOT_EXACT", "The control plane returned a value that cannot be represented exactly.");
    return BigInt(value);
  }
  if (!/^-?\d+$/.test(value)) throw new ApiError(500, "PLATFORM_METRIC_INVALID", "The control plane returned an invalid metric value.");
  return BigInt(value);
};

const total = (rows: Group[]) => String(rows.reduce((sum, row) => sum + exact(row.value), BigInt(0)));

export function requirePlatformExecutive(actor: PlatformContext) {
  if (!hasPermission(actor, "platform.executive.read")) throw new ApiError(403, "PLATFORM_EXECUTIVE_FORBIDDEN", "Your Platform role cannot read Executive insights.");
  return actor;
}

/** Reads the control plane only. It intentionally returns no Holding names, record IDs, or error details. */
export async function platformExecutiveSnapshot(
  actor: PlatformContext,
  control: SqlDatabase,
  controlRead: SqlDatabase,
  rawFilters?: unknown,
): Promise<PlatformExecutiveSnapshot> {
  requirePlatformExecutive(actor);
  const year = new Date().getUTCFullYear();
  const filters = dateFilters.parse(rawFilters ?? { from: `${year}-01-01`, to: new Date().toISOString().slice(0, 10) });
  const fromAt = Date.parse(filters.from);
  const toAt = Date.parse(filters.to) + 86_400_000;
  const [failed, abnormal] = await Promise.all([
    control.prepare(`SELECT to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label,count(*)::text value
      FROM platform_holding_provisioning_events WHERE action='failed' AND created_at>=? AND created_at<?
      GROUP BY 1 ORDER BY 1`).bind(fromAt, toAt).all<Group>(),
    controlRead.prepare(`SELECT component||' · '||status label,count(*)::text value
      FROM platform_health WHERE lower(status) NOT IN ('healthy','ready','ok')
      GROUP BY component,status ORDER BY component,status`).all<Group>(),
  ]);
  const sourceRows = {
    provisioningFailures: failed.results,
    abnormalServices: abnormal.results,
  };
  const sourceRevision = await sha256(JSON.stringify(sourceRows));
  const referencedAt = Date.now();
  const metrics: PlatformExecutiveSnapshot["metrics"] = [
    {
      id: "failed-provisioning",
      label: "Failed provisioning jobs",
      formula: "Count immutable platform_holding_provisioning_events with action failed in the selected UTC interval.",
      dateBasis: "Failure event time (UTC)",
      total: total(failed.results),
      rows: failed.results,
      status: failed.results.length ? "ready" : "empty",
      sourceReferences: [{ dataset: "platform_holding_provisioning_events", revision: sourceRevision, referencedAt }],
    },
    {
      id: "abnormal-platform-services",
      label: "Abnormal platform services",
      formula: "Count monitored platform_health components whose current status is outside healthy, ready, and ok.",
      dateBasis: "Latest health observation",
      total: total(abnormal.results),
      rows: abnormal.results,
      status: abnormal.results.length ? "ready" : "empty",
      sourceReferences: [{ dataset: "platform_health", revision: sourceRevision, referencedAt }],
    },
  ];
  return { scope: "platform", referenceAt: referencedAt, filters, sourceRevision, metrics };
}
