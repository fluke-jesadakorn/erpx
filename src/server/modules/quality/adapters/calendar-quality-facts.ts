import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {commercePackageEnforcementMode,readPurchasedAccess} from "../../commercial/public/purchase-command-boundary";
import {packageClockSql} from "../../commercial/public/purchased-capability-sql";
import {qualityPurchaseSourceRows} from "./purchase-sources";
import {qualitySourcePurchasePredicate} from "./record-purchases";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/commercial-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarQualityQualityFacts(db: SqlDatabase,actor?:WorkContext) { return { read1: async (facts: {
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
 const mode=await commercePackageEnforcementMode();if(mode!=="disabled"&&!actor)throw new Error("QUALITY_CALENDAR_ACTOR_REQUIRED");
 if(mode==="shadow"&&actor)await readPurchasedAccess(db,mode,{companyId:actor.companyId,capability:"supply.quality"});
 const enforced=mode==="enforced"&&actor,clock=enforced?`WITH quality_calendar_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) `:"",purchase=enforced?` AND EXISTS(SELECT 1 FROM (${qualityPurchaseSourceRows}) quality_calendar_source CROSS JOIN quality_calendar_clock WHERE quality_calendar_source.id=h.id AND quality_calendar_source.company_id=h.company_id AND ${qualitySourcePurchasePredicate("quality_calendar_source","history","quality_calendar_clock.now_ms")}) AND EXISTS(SELECT 1 FROM (SELECT h.company_id,h.branch_id,NULL::text department_id) quality_calendar_scope WHERE ${assignmentReadSql("quality_calendar_scope","quality.read")})`:"";
 return db.prepare(`${clock}SELECT h.id,h.due_date,h.status,h.version,h.branch_id,p.name product_name FROM inventory_quality_holds h JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND ${inList('h.branch_id')} AND h.due_date>=? AND h.due_date<? AND h.status NOT IN ('released','cancelled','disposed') AND (?='team' OR h.created_by=? OR h.last_inspector=?) AND (?::text IS NULL OR h.created_by IN (SELECT user_id FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE id=? AND company_id=h.company_id)) AND (?='' OR p.name ILIKE '%'||?||'%') ${purchase} ORDER BY h.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9,...(enforced?[actor.assignmentId,actor.userId,actor.role]:[]), facts.value10).all<{
        id: string;
        due_date: string;
        status: string;
        version: number;
        branch_id: string;
        product_name: string;
    }>().then(result => result.results); } }; }
