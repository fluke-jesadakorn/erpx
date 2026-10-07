import { z } from "zod";
import { breakdowns, filtersSchema, metricIds, type MetricData } from "@/server/modules/runtime/public/rules/canvas-domain";
const page = { limit: z.number().int().min(1).max(200).default(100), cursor: z.string().max(180).optional() };
const dashboardId = z.string().min(1).max(180);
export const executiveToolSchemas = {
    list_dashboards: z.object(page).strict(),
    get_dashboard: z.object({ dashboardId }).strict(),
    get_dashboard_data: z.object({ dashboardId, filters: filtersSchema.optional(), rowLimit: z.number().int().min(1).max(200).default(100) }).strict(),
    list_metrics: z.object({}).strict(),
    query_metric: z.object({ metric: z.enum(metricIds), filters: filtersSchema, breakdown: z.enum(breakdowns).default("default"), ...page }).strict(),
};
export type ToolName = keyof typeof executiveToolSchemas;
export function metricProjection(data: MetricData, limit: number, offset = 0) {
    return { ...data, valueAvailable: !["unavailable", "failed"].includes(data.status), rows: data.rows.slice(offset, offset + limit), pagination: { offset, returned: Math.min(limit, Math.max(0, data.rows.length - offset)), totalRows: data.rows.length, truncated: offset + limit < data.rows.length, nextCursor: offset + limit < data.rows.length ? String(offset + limit) : null } };
}
