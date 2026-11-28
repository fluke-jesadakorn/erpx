import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {procurementSourcePredicate} from "@erpx/procurement-server/adapters/record-purchases";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/people-payroll-server/public/platform-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {inList,branchDepartmentScope} from './read-tail-sql';
import {hrPersonIn} from './read-tail-r20';
import {calendarScopedListRead} from './read-tail-r34';
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
    }) => { const scope=await procurementSourcePredicate(db,actor,"r");return db.prepare(calendarScopedListRead('r.*','procurement_rfqs r',branchDepartmentScope("r"),inList('coalesce(r.department_id,r.branch_id,r.company_id)'),`r.status NOT IN ('ordered','cancelled') AND r.needed_date>=? AND r.needed_date<? AND (?='team' OR r.created_by=? OR r.submitted_by=? OR r.approved_by=?) AND (?::text IS NULL OR ${hrPersonIn(people_payrollPlatformSources.hr_people,"r.created_by","r")}) AND (?='' OR r.title ILIKE '%'||?||'%')`,scope.where,'r.needed_date,r.id')).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10,...scope.args, facts.value11).all<{
        id: string;
        title: string;
        needed_date: string;
        status: string;
        version: number;
        branch_id: string;
        department_id: string | null;
    }>().then(result => result.results); } }; }
