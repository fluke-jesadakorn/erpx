import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {assetSourcePredicate} from "@erpx/assets-server/public/record-purchases";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/people-payroll-server/public/final-root-owner-projection-sources";
import {maintenanceOwnerName} from "./maintenance-owner-name";
import {projectionLookup,subqueryHead} from "./projection-lookup";
import {aliasRowsSql} from "./read-tail-r30-alias-rows";
import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
const inList = (expression: string) => `${expression} IN ${subqueryHead("jsonb_array_elements_text(?::jsonb)")}`;
/** Canonical owner-local scheduled-source projection shared by the preventive
 * plan and work-order branches of the calendar read. Both branches project the
 * same owner columns into one scheduled shape; callers pass only owner-fixed
 * table, alias, due-at projection and a closed status predicate, never request-
 * authored SQL. Each composed branch stays byte-identical to its pre-dedupe
 * literal and no LOCK statement is accepted or changed. */
export const scheduledSourceSql = (
  alias: "p" | "w",
  sourceKind: "maintenance_plan" | "maintenance_work",
  dueAtSql: string,
  table: string,
  where: string,
): string =>
  `SELECT ${alias}.id,'${sourceKind}'${alias === "p" ? " source_kind" : ""},${alias}.company_id,${alias}.branch_id,${alias}.department_id,${alias}.title,${alias}.status,${alias}.version,${dueAtSql},${alias}.owner_id,${alias}.created_by
  FROM ${table} ${alias} WHERE ${where}`;
export function calendarMaintenanceMaintenanceFacts(db: SqlDatabase,actor?:WorkContext) {
    return { read1: async (facts: {
            value1: string;
            value2: string | null;
            value3: string | null;
            value4: string;
            value5: number;
            value6: number;
            value7: "mine" | "team";
            value8: string;
            value9: string | null;
            value10: string;
            value11: number;
        }) => {
            const work=actor?await assetSourcePredicate(db,actor,"s","maintenance_work"):{where:"TRUE",args:[]},plan=actor?await assetSourcePredicate(db,actor,"s","maintenance_plan"):{where:"TRUE",args:[]};
            return db.prepare(`WITH scheduled AS (
  ${scheduledSourceSql("p","maintenance_plan","p.next_due_at due_at","maintenance_preventive_plans","p.status='active'")}
  UNION ALL ${scheduledSourceSql("w","maintenance_work","w.scheduled_at","maintenance_work_orders","w.status NOT IN ('completed','cancelled') AND w.scheduled_at IS NOT NULL")})
  ${aliasRowsSql("s", `,${maintenanceOwnerName('s')} owner_name`)}
  FROM scheduled s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR s.department_id=?) AND ${inList('coalesce(s.department_id,s.branch_id)')}
  AND s.due_at>=? AND s.due_at<? AND (?='team' OR s.owner_id=? OR s.created_by=?)
  AND (?::text IS NULL OR s.owner_id IN ${projectionLookup("user_id",people_payrollProjectionSources.hr_people,"hr_people","id=? AND company_id=s.company_id")})
  AND (?='' OR s.title ILIKE '%'||?||'%') AND ((s.source_kind='maintenance_work' AND ${work.where}) OR (s.source_kind='maintenance_plan' AND ${plan.where})) ORDER BY s.due_at,s.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10,...work.args,...plan.args, facts.value11).all<{
                id: string;
                source_kind: string;
                title: string;
                branch_id: string;
                department_id: string | null;
                status: string;
                version: number;
                due_at: number;
                owner_name: string | null;
            }>().then(result => result.results);
        } };
}
