import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {procurementSourcePredicate} from "@erpx/server-procurement/adapters/record-purchases";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarProcurementRfqsProcurementFacts(db: SqlDatabase,actor?:WorkContext) { return { read1: async (facts: {
        value1: string;
        value2: string | null;
        value3: string | null;
        value4: string;
        value5: string;
        value6: string;
        value7: "mine" | "team";
        value8: string;
        value9: string | null;
        value10: string;
        value11: number;
    }) => { const scope=await procurementSourcePredicate(db,actor,"r");return db.prepare(`SELECT r.* FROM procurement_rfqs r WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) AND ${inList('coalesce(r.department_id,r.branch_id,r.company_id)')} AND r.status NOT IN ('ordered','cancelled') AND r.needed_date>=? AND r.needed_date<? AND (?='team' OR r.created_by=? OR r.submitted_by=? OR r.approved_by=?) AND (?::text IS NULL OR r.created_by IN(SELECT user_id FROM (${people_payrollPlatformSources.hr_people}) hr_people WHERE id=? AND company_id=r.company_id)) AND (?='' OR r.title ILIKE '%'||?||'%') AND ${scope.where} ORDER BY r.needed_date,r.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10,...scope.args, facts.value11).all<{
        id: string;
        title: string;
        needed_date: string;
        status: string;
        version: number;
        branch_id: string;
        department_id: string | null;
    }>().then(result => result.results); } }; }
