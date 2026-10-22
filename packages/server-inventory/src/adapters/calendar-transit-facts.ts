import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {commercePackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import { inventoryExecutiveOwnerProjections as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/inventory-executive-owner-projections";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarTransitInventoryFacts(db: SqlDatabase,actor?:WorkContext) { return { read1: async (facts: {
        value1: string;
        value2: string | null;
        value3: string;
        value4: string;
        value5: string;
        value6: "mine" | "team";
        value7: string;
        value8: string | null;
        value9: string;
        value10: number;
    }) => {
 const mode=await commercePackageEnforcementMode();
 if(mode!=="disabled"&&!actor)throw new Error("TRANSIT_CALENDAR_ACTOR_REQUIRED");
 if(mode==="shadow"&&actor)await readPurchasedAccess(db,mode,{companyId:actor.companyId,capability:"supply.transfers"});
 const enforced=mode==="enforced"&&actor;
 const scope=enforced?` AND ${purchasedCapabilityPredicate("d.company_id","supply.transfers","history","transit_calendar_clock.now_ms")} AND ${assignmentReadSql("transit_calendar_scope","inventory.read")}`:"";
 const clock=enforced?`WITH transit_calendar_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) `:"";
 const authority=enforced?" CROSS JOIN transit_calendar_clock CROSS JOIN (SELECT ?::text company_id,?::text branch_id,NULL::text department_id) transit_calendar_scope":"";
 return db.prepare(`${clock}SELECT d.id,t.number,d.due_date,d.status,d.version,d.target_branch_id FROM inventory_transit d JOIN inventory_transfers t ON t.id=d.transfer_id AND t.company_id=d.company_id AND t.source_warehouse_id=d.source_warehouse_id AND t.target_warehouse_id=d.target_warehouse_id AND t.source_branch_id=d.source_branch_id AND t.target_branch_id=d.target_branch_id AND t.delivery_mode='transit'${authority} WHERE d.company_id=? AND (?::text IS NULL OR d.source_branch_id=? OR d.target_branch_id=?) AND (${inList('d.source_branch_id')} OR ${inList('d.target_branch_id')}) AND d.due_date>=? AND d.due_date<? AND d.status NOT IN ('completed','cancelled') AND (?='team' OR d.created_by=? OR d.approved_by=? OR d.dispatched_by=?) AND (?::text IS NULL OR d.created_by IN (SELECT user_id FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE id=? AND company_id=d.company_id)) AND (?='' OR t.number ILIKE '%'||?||'%') ${scope} ORDER BY d.id LIMIT ?`).bind(...(enforced?[actor.companyId,actor.branchId]:[]),facts.value1, facts.value2, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value7, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9,...(enforced?[actor.assignmentId,actor.userId,actor.role]:[]), facts.value10).all<{
        id: string;
        number: string;
        due_date: string;
        status: string;
        version: number;
        target_branch_id: string;
    }>().then(result => result.results); } }; }
