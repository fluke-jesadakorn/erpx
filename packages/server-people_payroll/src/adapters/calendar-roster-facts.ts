import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { peoplePackageEnforcementMode } from "@erpx/server-commercial/public/purchase-command-boundary";
import { packageClockSql, purchasedCapabilityPredicate } from "@erpx/server-commercial/public/purchased-capability-sql";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
async function purchasedRosterSql() {
    const mode = await peoplePackageEnforcementMode();
    return mode === "enforced" ? {
        clock: `WITH roster_purchase_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) `,
        where: ` AND (SELECT ${purchasedCapabilityPredicate("w.company_id", "people.workforce", "history", "roster_purchase_clock.now_ms")} FROM roster_purchase_clock)`,
    } : {clock: "", where: ""};
}
export function calendarRosterPeoplePayrollFacts(db: SqlDatabase) {
    return { read1: async (facts: {
            value1: string;
            value2: string;
            value3: boolean;
            value4: number;
            value5: number;
            value6: string | null;
            value7: string;
            value8: number;
        }) => {
            const purchase = await purchasedRosterSql();
            return db.prepare(`${purchase.clock}SELECT s.id,s.name,s.local_date,s.starts_at,s.ends_at,s.capacity,s.version,s.week_id,w.week_start,w.timezone,w.state,w.department_id,w.branch_id,coalesce(p.legal_name,'Roster manager') owner_name
 FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id LEFT JOIN hr_people p ON p.user_id=w.created_by AND p.company_id=w.company_id AND p.merged_into_id IS NULL
 WHERE w.company_id=? AND ${inList("w.department_id")} AND (? OR w.state='published') AND s.state='active' AND s.starts_at<? AND s.ends_at>?
 AND (?::text IS NULL OR EXISTS(SELECT 1 FROM people_roster_assignments ra WHERE ra.shift_id=s.id AND ra.state='assigned' AND ra.person_id=?))
 AND (?='' OR s.name ILIKE '%'||?||'%' OR EXISTS(SELECT 1 FROM people_roster_assignments ra JOIN hr_people hp ON hp.id=ra.person_id WHERE ra.shift_id=s.id AND ra.state='assigned' AND hp.legal_name ILIKE '%'||?||'%'))${purchase.where} ORDER BY s.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value6, facts.value7, facts.value7, facts.value7, facts.value8).all<{
                id: string;
                name: string;
                local_date: string;
                starts_at: number;
                ends_at: number;
                capacity: number;
                version: number;
                week_id: string;
                week_start: string;
                timezone: string;
                state: string;
                department_id: string;
                branch_id: string | null;
                owner_name: string;
            }>().then(result => result.results);
        },
        read2: async (facts: {
            value1: string;
        }) => { const purchase = await purchasedRosterSql(); return db.prepare(`${purchase.clock}SELECT ra.shift_id,p.id,p.legal_name name FROM people_roster_assignments ra JOIN people_roster_shifts s ON s.id=ra.shift_id JOIN people_roster_weeks w ON w.id=s.week_id JOIN hr_people p ON p.id=ra.person_id AND p.company_id=w.company_id WHERE ra.state='assigned' AND ${inList("ra.shift_id")}${purchase.where} ORDER BY p.legal_name`).bind(facts.value1).all<{
            shift_id: string;
            id: string;
            name: string;
        }>().then(result => result.results); } };
}
