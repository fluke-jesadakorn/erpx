import {manufacturingMetricSource} from "@erpx/server-manufacturing/public/metric-purchases";
import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "@erpx/server-commerce/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as manufacturingProjectionSources } from "@erpx/server-manufacturing/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as organizationProjectionSources } from "@erpx/server-organization/public/inventory-executive-owner-projections";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { inputSchema, type EvidenceRow, parseCursor } from '@erpx/server-runtime/public/executive/planning-evidence-contract';
const sourceFrom = `FROM planning_scenarios s
  LEFT JOIN (${manufacturingProjectionSources.manufacturing_specifications}) spec ON spec.id=s.specification_id AND spec.company_id=s.company_id
  LEFT JOIN (${commerceProjectionSources.commerce_products}) output ON output.id=spec.output_product_id AND output.company_id=s.company_id
  LEFT JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=s.warehouse_id AND w.company_id=s.company_id
  LEFT JOIN (${organizationProjectionSources.organization_nodes}) branch ON branch.id=s.branch_id AND branch.company_id=s.company_id
  LEFT JOIN planning_recommendations r ON r.scenario_id=s.id
  LEFT JOIN LATERAL (
    SELECT count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count
    FROM jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)
  ) shortage ON TRUE`;
const commonSelect = `s.id scenario_id,s.branch_id,branch.display_name branch_name,s.warehouse_id,w.name warehouse_name,
  s.specification_id,spec.name specification_name,spec.output_product_id,output.name output_product_name,s.specification_version,
  s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,
  s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,
  r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,
  r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,r.scenario_version scenario_version_ref,
  r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,COALESCE(shortage.shortage_count,'0') shortage_count`;
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>, isScenario: boolean): Promise<EvidenceRow[]> {
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
            const purchased=await manufacturingMetricSource(db,actor,input.metric);
            const where = ["s.company_id=?", "(?::text IS NULL OR s.branch_id=?)",`EXISTS(SELECT 1 FROM (${purchased.sql}) purchased_production WHERE purchased_production.id=s.id)`];
            values.push(...purchased.args);
            if (isScenario) {
                where.push("s.due_date BETWEEN ? AND ?", "jsonb_path_exists(COALESCE(s.shortages_json::jsonb,'[]'::jsonb),'$[*] ? (@.shortageMilli > 0)')");
            }
            else {
                where.push("r.feasible_date BETWEEN ? AND ?", input.metric === "proposed-planning-recommendations"
                    ? "r.status='proposed'"
                    : "r.status IN ('proposed','approved') AND r.required_minutes<=s.capacity_minutes");
            }
            values.push(input.filters.from, input.filters.to);
            if (input.query) {
                where.push(`(position(lower(?) in lower(COALESCE(s.id,'')))>0 OR position(lower(?) in lower(COALESCE(spec.name,'')))>0
      OR position(lower(?) in lower(COALESCE(w.name,'')))>0 OR position(lower(?) in lower(COALESCE(branch.display_name,'')))>0
      OR position(lower(?) in lower(COALESCE(s.status,'')))>0 OR position(lower(?) in lower(COALESCE(r.status,'')))>0
      OR position(lower(?) in lower(COALESCE(s.due_date,'')))>0 OR position(lower(?) in lower(COALESCE(r.feasible_date,'')))>0)`);
                values.push(...Array(8).fill(input.query));
            }
            if (cursor) {
                where.push(isScenario ? "s.id>?" : "r.id>?");
                values.push(cursor.id);
            }
            const rows = (await db.prepare(`SELECT ${isScenario ? "s.id" : "r.id"} id,${commonSelect}
    ${sourceFrom} WHERE ${where.join(" AND ")} ORDER BY ${isScenario ? "s.id" : "r.id"} LIMIT ?`)
                .bind(...values, input.limit + 1).all<EvidenceRow>()).results;
            return rows;
        } };
}
