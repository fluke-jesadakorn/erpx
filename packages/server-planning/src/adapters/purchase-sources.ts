import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Persisted comparison identity, not an MRP route or a client label, owns the
 * advanced operation. A malformed comparison never becomes a basic plan. */
export const planningPurchaseScenarioRows=`SELECT s.id,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,
 (COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')<>'' OR jsonb_exists(s.context_json::jsonb,'comparison')) advanced,
 jsonb_build_object('scenario',to_jsonb(s)-'status'-'version'-'updated_at','comparison',CASE WHEN comparison.id IS NULL THEN NULL ELSE jsonb_build_array(comparison.id,comparison.company_id,comparison.branch_id,comparison.specification_id,comparison.version) END) witness
 FROM planning_scenarios s LEFT JOIN planning_scenarios comparison ON comparison.id=s.context_json::jsonb->>'comparisonScenarioId'
 AND comparison.id=s.context_json::jsonb->'comparison'->>'scenarioId' AND comparison.company_id=s.company_id AND comparison.branch_id=s.branch_id AND comparison.specification_id=s.specification_id
 AND comparison.version>=(s.context_json::jsonb->'comparison'->>'sourceVersion')::bigint
 WHERE (COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')='' AND NOT(jsonb_exists(s.context_json::jsonb,'comparison'))) OR comparison.id IS NOT NULL`;
export const planningPurchaseRecommendationRows=`SELECT r.id,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.advanced,
 jsonb_build_object('recommendation',to_jsonb(r)-'status'-'version'-'updated_at'-'reviewed_by','scenario',s.witness) witness
 FROM planning_recommendations r JOIN (${planningPurchaseScenarioRows}) s ON s.id=r.scenario_id`;
export const planningPurchaseResourceRows=`SELECT id,company_id,branch_id,false advanced,jsonb_build_array(id,company_id,branch_id,created_by,created_at) witness FROM production_resources`;
export function planningPurchaseSourceReadLocks(database:SqlDatabase,writer=false){return [database.prepare(`LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE`)];}
