import {workforcePurchasedHistorySql} from "./workforce-purchases";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import "server-only";
export function SelfServicePeoplePayrollOwnedFacts(database: SqlDatabase) {
    return { async readSelfServiceSummarySources(companyId: string, engagement_id: string, now: number) {
            return database.prepare(`SELECT s.id,s.name,s.local_date,s.start_time,s.end_time,s.starts_at,s.ends_at,w.timezone FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id JOIN people_roster_weeks w ON w.id=s.week_id WHERE w.company_id=? AND ${await workforcePurchasedHistorySql("w.company_id")} AND a.engagement_id=? AND w.state='published' AND a.state='assigned' AND s.state='active' AND s.ends_at>? ORDER BY s.starts_at LIMIT 1`).bind(companyId, engagement_id, now).first<{
                id: string;
                name: string;
                local_date: string;
                start_time: string;
                end_time: string;
                starts_at: number;
                ends_at: number;
                timezone: string;
            }>();
        } };
}
