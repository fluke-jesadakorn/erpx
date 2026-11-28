import { readMetricSources } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { specs } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import {
  BRAND_RD_METRIC_PROFILES_BRANCH,
  BRAND_RD_METRIC_PROFILES_LABEL,
  BRAND_RD_METRIC_UPDATES_BRANCH,
  BRAND_RD_METRIC_UPDATES_MONTH,
  planBrandRead,
} from "./native-brand-read-ports";
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    const updates = id === "brand-profile-updates";
    const kind = updates
        ? (breakdown === "branch" ? BRAND_RD_METRIC_UPDATES_BRANCH : BRAND_RD_METRIC_UPDATES_MONTH)
        : (breakdown === "branch" ? BRAND_RD_METRIC_PROFILES_BRANCH : BRAND_RD_METRIC_PROFILES_LABEL);
    const values: unknown[] = [a.companyId, a.branchId, a.branchId, ...(updates ? [Date.parse(f.from), Date.parse(f.to) + 86400000] : [])];
    const plan = await planBrandRead(kind, values, {}, a);
    const raw = (await sourceDb.prepare(plan.sql).bind(...plan.values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride: null, source: (group, reported) => readMetricSources(sourceDb, id, plan.sql, plan.values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
