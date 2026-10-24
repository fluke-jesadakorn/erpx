import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import "server-only";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {supportTicketReadPredicate} from '@erpx/server-support/public/support-ticket-read';
import {supportIncidentReadPredicate} from '@erpx/server-support/public/support-incident-read';
export function calendarSupportSupportFacts(db: SqlDatabase) {
    return { read1: async (facts: {
            value1: string;
            value2: string;
            value3: string | null;
            value4: string | null;
            value5: string;
            value6: string;
            value7: string;
            value8: "mine" | "team";
            value9: string;
            value10: string | null;
            value11: string;
            value12: number;
            actor: WorkContext;
        }) => { if (!(await nativeSupportRuntime(facts.actor, db).purchase("support.tickets")).history) return []; const permission = supportTicketReadPredicate(facts.actor); return db.prepare(`WITH tickets AS(SELECT s.*,CASE WHEN s.sla_clock_json->>'state'='running' THEN to_char(to_timestamp((s.sla_clock_json->>'dueAt')::bigint/1000.0) AT TIME ZONE ?,'YYYY-MM-DD') ELSE s.due_date END target_date FROM support_tickets s) SELECT t.id,t.title,t.source_record_json,t.target_date due_date,CASE WHEN t.sla_clock_json->>'state'='running' THEN (t.sla_clock_json->>'dueAt')::bigint ELSE NULL END sla_due_at,t.status,t.version,t.branch_id,t.department_id,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=t.company_id AND user_id=t.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name FROM tickets t WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${inList('coalesce(t.department_id,t.branch_id,t.company_id)')} AND ${permission.sql} AND t.status NOT IN ('closed','cancelled') AND t.target_date>=? AND t.target_date<? AND (?='team' OR t.owner_id=? OR t.created_by=?) AND (?::text IS NULL OR t.owner_id IN(SELECT user_id FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE id=? AND company_id=t.company_id)) AND (?='' OR t.title ILIKE '%'||?||'%') ORDER BY t.target_date,t.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value3, facts.value3, facts.value4, facts.value4, facts.value5, ...permission.bindings, facts.value6, facts.value7, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10, facts.value11, facts.value11, facts.value12).all<{
            id: string;
            title: string;
            due_date: string;
            status: string;
            version: number;
            branch_id: string;
            department_id: string | null;
            owner_name: string | null;
            sla_due_at: number | null;
            source_record_json: string | null;
        }>().then(result => result.results); },
        read2: async (facts: {
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
            actor: WorkContext;
        }) => { if (!(await nativeSupportRuntime(facts.actor, db).purchase("support.incidents")).history) return []; const permission = supportIncidentReadPredicate(facts.actor); return db.prepare(`SELECT t.id,t.title,t.due_date,t.status,t.version,t.branch_id,t.department_id FROM support_incidents t WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${inList('coalesce(t.department_id,t.branch_id,t.company_id)')} AND ${permission.sql} AND t.status NOT IN ('closed','cancelled') AND t.due_date>=? AND t.due_date<? AND (?='team' OR t.owner_id=? OR t.created_by=?) AND (?::text IS NULL OR t.owner_id IN(SELECT user_id FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE id=? AND company_id=t.company_id)) AND (?='' OR t.title ILIKE '%'||?||'%') ORDER BY t.due_date,t.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, ...permission.bindings, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10, facts.value11).all<{
            id: string;
            title: string;
            due_date: string;
            status: string;
            version: number;
            branch_id: string;
            department_id: string | null;
        }>().then(result => result.results); } };
}
