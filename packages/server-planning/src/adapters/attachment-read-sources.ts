import 'server-only';
/** Closed full native planning headers and actual optional child lineage for
 * private document consistency. No caller SQL, permission or client snapshot. */
export const planningAttachmentScenarioSourceRows=`SELECT scenario.id,scenario.company_id,scenario.specification_id,scenario.warehouse_id,
 jsonb_build_object('scenario',to_jsonb(scenario),
 'comparison',(SELECT to_jsonb(comparison) FROM planning_scenarios comparison WHERE comparison.id=CASE WHEN scenario.context_json IS JSON OBJECT THEN scenario.context_json::jsonb->>'comparisonScenarioId' END AND comparison.company_id=scenario.company_id),
 'recommendation',(SELECT to_jsonb(recommendation) FROM planning_recommendations recommendation WHERE recommendation.scenario_id=scenario.id),
 'resources',(SELECT jsonb_agg(jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),
 'calendar',(SELECT jsonb_agg(to_jsonb(calendar) ORDER BY calendar.work_date) FROM production_resource_calendar calendar WHERE calendar.resource_id=resource.id)) ORDER BY allocation.id)
 FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id WHERE allocation.scenario_id=scenario.id)) image
 FROM planning_scenarios scenario`;
export const planningAttachmentRecommendationSourceRows=`SELECT recommendation.id,recommendation.scenario_id,scenario.company_id,
 jsonb_build_object('recommendation',to_jsonb(recommendation),'scenario',scenario.image) image
 FROM planning_recommendations recommendation LEFT JOIN (${planningAttachmentScenarioSourceRows}) scenario ON scenario.id=recommendation.scenario_id`;
