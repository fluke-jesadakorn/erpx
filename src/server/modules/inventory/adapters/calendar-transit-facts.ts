import { inventoryExecutiveOwnerProjections as people_payrollProjectionSources } from "../../people_payroll/public/inventory-executive-owner-projections";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarTransitInventoryFacts(db: SqlDatabase) { return { read1: (facts: {
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
    }) => { return db.prepare(`SELECT d.id,t.number,d.due_date,d.status,d.version,d.target_branch_id FROM inventory_transit d JOIN inventory_transfers t ON t.id=d.transfer_id WHERE d.company_id=? AND (?::text IS NULL OR d.source_branch_id=? OR d.target_branch_id=?) AND (${inList('d.source_branch_id')} OR ${inList('d.target_branch_id')}) AND d.due_date>=? AND d.due_date<? AND d.status NOT IN ('completed','cancelled') AND (?='team' OR d.created_by=? OR d.approved_by=? OR d.dispatched_by=?) AND (?::text IS NULL OR d.created_by IN (SELECT user_id FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE id=? AND company_id=d.company_id)) AND (?='' OR t.number ILIKE '%'||?||'%') ORDER BY d.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value7, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10).all<{
        id: string;
        number: string;
        due_date: string;
        status: string;
        version: number;
        target_branch_id: string;
    }>().then(result => result.results); } }; }
