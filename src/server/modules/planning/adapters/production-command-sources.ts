import {productionReadProjection as productionmanufacturingProjection} from '../../manufacturing/public/production-read-projections';
import {productionReadProjection as productioncommerceProjection} from '../../commerce/public/production-read-projections';
import {productionReadProjection as productionorganizationProjection} from '../../organization/public/production-read-projections';
import type {SqlDatabase} from '../../runtime/public/runtime/types';import type {Row} from '../../commerce/public/commerce';import {assertion} from '../../workflow/public/commands';export function productionCommandSources(db:SqlDatabase){return {async scenarios(companyId:string,branchId:string | null,branchId2:string | null){return db
          .prepare(
            `SELECT p.*,s.name specification_name FROM planning_scenarios p ${productionmanufacturingProjection.scenarioSpecification} WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) ORDER BY p.updated_at DESC`,
          )
          .bind(companyId, branchId, branchId2)
          .all<Row>();},
async recommendations(companyId:string,branchId:string | null,branchId2:string | null){return db
          .prepare(
            "SELECT r.*,s.company_id,s.branch_id FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY r.updated_at DESC",
          )
          .bind(companyId, branchId, branchId2)
          .all<Row>();},
async resources(companyId:string,branchId:string | null,branchId2:string | null){return db.prepare("SELECT * FROM production_resources WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name")
          .bind(companyId, branchId, branchId2).all<Row>();},
async calendar(companyId:string,branchId:string | null,branchId2:string | null){return db.prepare(`SELECT c.* FROM production_resource_calendar c JOIN production_resources r ON r.id=c.resource_id
          WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY c.work_date DESC LIMIT 500`)
          .bind(companyId, branchId, branchId2).all<Row>();},
async executiveShortage(recordId:string,companyId:string,branchId:string | null,branchId2:string | null,from:string,to:string){const branchScope = "(?::text IS NULL OR s.branch_id=?)";
const sourceJoins = `
    ${productionmanufacturingProjection.executiveSpecification}
    ${productioncommerceProjection.executiveOutput}
    ${productioncommerceProjection.executiveWarehouse}
    ${productionorganizationProjection.executiveBranch}
    LEFT JOIN planning_recommendations r ON r.scenario_id=s.id`;
const shortageProjection = `
    LEFT JOIN LATERAL (
      SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'product_name',COALESCE(p.name,'Unknown product'),
        'required_milli',(entry.value->>'requiredMilli')::numeric::text,
        'available_milli',(entry.value->>'availableMilli')::numeric::text,
        'shortage_milli',(entry.value->>'shortageMilli')::numeric::text
      ) ORDER BY COALESCE(p.name,entry.value->>'productId')) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0),'[]'::jsonb)::text shortage_items_json,
      count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count
      FROM jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)
      ${productioncommerceProjection.executiveShortageProduct}
    ) shortage ON TRUE`;
const projection = `SELECT s.id scenario_id,s.company_id,s.branch_id,branch.display_name branch_name,
      s.warehouse_id,w.name warehouse_name,s.specification_id,spec.name specification_name,
      spec.output_product_id,output.name output_product_name,s.specification_version,
      s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,
      s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,
      r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,
      r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,
      r.scenario_version,r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,
      COALESCE(shortage.shortage_count,'0') shortage_count,COALESCE(shortage.shortage_items_json,'[]') shortage_items_json
    FROM planning_scenarios s ${sourceJoins} ${shortageProjection}`;return db.prepare(`${projection}
      WHERE s.id=? AND s.company_id=? AND ${branchScope} AND s.due_date BETWEEN ? AND ?
        AND jsonb_path_exists(COALESCE(s.shortages_json::jsonb,'[]'::jsonb),'$[*] ? (@.shortageMilli > 0)') LIMIT 1`)
      .bind(recordId,companyId,branchId,branchId2,from,to).first<Row>();},
async executiveRecommendation(recordId:string,companyId:string,branchId:string | null,branchId2:string | null,from:string,to:string,eligibilityKind:"proposed"|"capacity"){const branchScope = "(?::text IS NULL OR s.branch_id=?)";
const sourceJoins = `
    ${productionmanufacturingProjection.executiveSpecification}
    ${productioncommerceProjection.executiveOutput}
    ${productioncommerceProjection.executiveWarehouse}
    ${productionorganizationProjection.executiveBranch}
    LEFT JOIN planning_recommendations r ON r.scenario_id=s.id`;
const shortageProjection = `
    LEFT JOIN LATERAL (
      SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'product_name',COALESCE(p.name,'Unknown product'),
        'required_milli',(entry.value->>'requiredMilli')::numeric::text,
        'available_milli',(entry.value->>'availableMilli')::numeric::text,
        'shortage_milli',(entry.value->>'shortageMilli')::numeric::text
      ) ORDER BY COALESCE(p.name,entry.value->>'productId')) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0),'[]'::jsonb)::text shortage_items_json,
      count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count
      FROM jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)
      ${productioncommerceProjection.executiveShortageProduct}
    ) shortage ON TRUE`;
const projection = `SELECT s.id scenario_id,s.company_id,s.branch_id,branch.display_name branch_name,
      s.warehouse_id,w.name warehouse_name,s.specification_id,spec.name specification_name,
      spec.output_product_id,output.name output_product_name,s.specification_version,
      s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,
      s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,
      r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,
      r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,
      r.scenario_version,r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,
      COALESCE(shortage.shortage_count,'0') shortage_count,COALESCE(shortage.shortage_items_json,'[]') shortage_items_json
    FROM planning_scenarios s ${sourceJoins} ${shortageProjection}`;const eligibility=eligibilityKind==='proposed'?"r.status='proposed'":"r.status IN ('proposed','approved') AND r.required_minutes<=s.capacity_minutes";return db.prepare(`${projection}
    WHERE r.id=? AND s.company_id=? AND ${branchScope} AND r.feasible_date BETWEEN ? AND ? AND ${eligibility} LIMIT 1`)
    .bind(recordId,companyId,branchId,branchId2,from,to).first<Row>();},
guardRoutingResource(key:string,value:string,resourceId:string,companyId:string,branchId:string | null,branchId2:string | null){return assertion(db, key, value, "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?))", [resourceId, companyId, branchId, branchId2]);},
async comparison(comparisonScenarioId:string,companyId:string,branch_id:string | number | null,specificationId:string){return db.prepare(`SELECT s.id,s.version,s.shortages_json,r.required_minutes,r.feasible_date FROM planning_scenarios s
      JOIN planning_recommendations r ON r.scenario_id=s.id WHERE s.id=? AND s.company_id=? AND s.branch_id=? AND s.specification_id=?`)
      .bind(comparisonScenarioId, companyId, branch_id, specificationId).first<Row>();},
guardComparison(key:string,value:"comparison",comparisonScenarioId:string,companyId:string,value2:string,specificationId:string,value3:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND branch_id=? AND specification_id=? AND version=?)", [comparisonScenarioId, companyId, value2, specificationId, value3]);},
prepareScenario(id:string,companyId:string,branch_id:string | number | null,warehouseId:string,specificationId:string,specificationVersion:number,demand:number,dueDate:string,capacityMinutes:number,value:string,userId:string,now:number,now2:number,value2:string){return db.prepare(`INSERT INTO planning_scenarios(id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,version,created_at,updated_at,context_json)
      VALUES(?,?,?,?,?,?,?,?,?,?,'calculated',?,1,?,?,?)`).bind(id, companyId, branch_id, warehouseId, specificationId,
        specificationVersion, demand, dueDate, capacityMinutes, value, userId, now, now2, value2);},
prepareRecommendation(recommendationId:string,id:string,value:1,demand:number,requiredMinutes:number,feasible:string,userId:string,now:number,now2:number){return db.prepare("INSERT INTO planning_recommendations VALUES(?,?,?,?,?,?,'proposed',?,NULL,1,?,?)")
      .bind(recommendationId, id, value, demand, requiredMinutes, feasible, userId, now, now2);},
prepareCalculateEvent(value:string,id:string,userId:string,value2:"Calculated from confirmed Sales demand and captured supply/capacity" | "Calculated from declared manual demand and captured supply/capacity",now:number){return db.prepare("INSERT INTO planning_scenario_events VALUES(?,?,1,'calculate',?,?,?)")
      .bind(value, id, userId, value2, now);},
async scenario(id:string,companyId:string,branchId:string | null,branchId2:string | null){return db.prepare("SELECT * FROM planning_scenarios WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)")
    .bind(id, companyId, branchId, branchId2).first<Row>();},
lockScenarioRecommendations(id:string){return db.prepare("SELECT id FROM planning_recommendations WHERE scenario_id=? FOR UPDATE").bind(id);},
guardUnreleasedScenario(key:string,value:"scenario",id:string,companyId:string,expectedVersion:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND version=? AND status='calculated')", [id, companyId, expectedVersion]);},
prepareScenarioCancellation(now:number,id:string){return db.prepare("UPDATE planning_scenarios SET status='cancelled',version=version+1,updated_at=? WHERE id=?").bind(now, id);},
prepareRecommendationCancellation(userId:string,now:number,id:string){return db.prepare("UPDATE planning_recommendations SET status='rejected',reviewed_by=?,version=version+1,updated_at=? WHERE scenario_id=? AND status='proposed'").bind(userId, now, id);},
prepareCancelEvent(value:string,id:string,value2:number,userId:string,reason:string,now:number){return db.prepare("INSERT INTO planning_scenario_events VALUES(?,?,?,'cancel',?,?,?)").bind(value, id, value2, userId, reason, now);},
async scopedRecommendation(id:string,companyId:string,branchId:string | null,branchId2:string | null){return db
      .prepare(
        "SELECT r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by scenario_created_by FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)",
      )
      .bind(id, companyId, branchId, branchId2)
      .first<Row>();},
lockRecommendation(id:string){return db.prepare("SELECT id FROM planning_recommendations WHERE id=? FOR UPDATE").bind(id);},
guardIndependentRecommendation(key:string,value:"recommendation",id:string,expectedVersion:number,userId:string){return assertion(
        db,
        key,
        value,
        "EXISTS(SELECT 1 FROM planning_recommendations WHERE id=? AND version=? AND status='proposed' AND created_by<>?)",
        [id, expectedVersion, userId],
      );},
prepareRecommendationReview(status:"approved" | "rejected",userId:string,now:number,id:string){return db
        .prepare(
          "UPDATE planning_recommendations SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(status, userId, now, id);},
guardCapacityReservation(key:string,value:string,resourceId:string,workDate:string,minutes:number,availableMinutes:number){return assertion(db, key, value, "COALESCE((SELECT sum(minutes) FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)+?<=?", [resourceId, workDate, minutes, availableMinutes]);},
prepareCapacityReservation(value:string,scenario_id:string | number | null,resourceId:string,workDate:string,minutes:number,userId:string,now:number){return db.prepare("INSERT INTO planning_resource_allocations(id,scenario_id,resource_id,work_date,minutes,created_by,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(value, scenario_id, resourceId, workDate, minutes, userId, now);},
prepareScenarioRelease(now:number,scenario_id:string | number | null){return db
        .prepare(
          "UPDATE planning_scenarios SET status='released',version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, scenario_id);},
prepareReleaseEvent(value:string,userId:string,reason:string,now:number,scenario_id:string | number | null){return db.prepare("INSERT INTO planning_scenario_events(id,scenario_id,version,action,actor_id,reason,created_at) SELECT ?,id,version,'release',?,?,? FROM planning_scenarios WHERE id=?")
        .bind(value, userId, reason, now, scenario_id);},
prepareCapacityRelease(now:number,id:string){return db.prepare(`UPDATE planning_resource_allocations SET released_at=? WHERE scenario_id=
    (SELECT r.scenario_id FROM planning_recommendations r ${productionmanufacturingProjection.resourceReleaseOrder} WHERE o.id=?) AND released_at IS NULL`).bind(now, id);}};}
