import {financeSnapshotSources} from "../../people_payroll/public/finance-snapshot-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarSupplierBillsProcurementFacts(db: SqlDatabase) { return { read1: (facts: {
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
    }) => { return db.prepare(`SELECT r.* FROM procurement_bill_matches r WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) AND ${inList('coalesce(r.department_id,r.branch_id,r.company_id)')} AND (r.status IN ('review','approved') OR (r.status='returned' AND r.id=(SELECT latest.id FROM procurement_bill_matches latest WHERE latest.fulfillment_id=r.fulfillment_id ORDER BY (latest.status<>'returned') DESC,latest.created_at DESC,latest.id DESC LIMIT 1))) AND r.due_date>=? AND r.due_date<? AND (?='team' OR r.created_by=? OR r.reviewed_by=?) AND (?::text IS NULL OR ${financeSnapshotSources().billPerson}) AND (?='' OR r.reference ILIKE '%'||?||'%') ORDER BY r.due_date,r.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10, facts.value11).all<{
        id: string;
        reference: string;
        due_date: string;
        status: string;
        version: number;
        branch_id: string;
        department_id: string | null;
    }>().then(result => result.results); } }; }
