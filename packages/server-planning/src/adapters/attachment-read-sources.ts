import 'server-only';
import {planningJsonbRowByKey} from './planning-jsonb-row-sql';
import {planningJsonbAggOrdered} from './planning-jsonb-agg-sql';
/** Closed full native planning headers and actual optional child lineage for
 * private document consistency. No caller SQL, permission or client snapshot. */
export const planningAttachmentScenarioSourceRows=`SELECT scenario.id,scenario.company_id,scenario.specification_id,scenario.warehouse_id,
 jsonb_build_object('scenario',to_jsonb(scenario),
 'comparison',${planningJsonbRowByKey('planning_scenarios','comparison',"comparison.id=CASE WHEN scenario.context_json IS JSON OBJECT THEN scenario.context_json::jsonb->>'comparisonScenarioId' END AND comparison.company_id=scenario.company_id")},
 'recommendation',${planningJsonbRowByKey('planning_recommendations','recommendation','recommendation.scenario_id=scenario.id')},
 'resources',${planningJsonbAggOrdered(`jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),\n 'calendar',${planningJsonbAggOrdered('to_jsonb(calendar)','calendar.work_date',' FROM production_resource_calendar calendar','calendar.resource_id=resource.id')})`,'allocation.id','\n FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id','allocation.scenario_id=scenario.id')}) image
 FROM planning_scenarios scenario`;
export const planningAttachmentRecommendationSourceRows=`SELECT recommendation.id,recommendation.scenario_id,scenario.company_id,
 jsonb_build_object('recommendation',to_jsonb(recommendation),'scenario',scenario.image) image
 FROM planning_recommendations recommendation LEFT JOIN (${planningAttachmentScenarioSourceRows}) scenario ON scenario.id=recommendation.scenario_id`;
