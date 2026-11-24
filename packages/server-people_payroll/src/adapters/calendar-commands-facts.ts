import {ppRd36TwoPredTail} from './pp-read-tail-r36-sources';
import {nativeCalendarRetirementNotification} from "./native-calendar-retirement-notification";
import {ppRd32TwoColProjection} from './pp-read-tail-r32-sources';
import {ppRd40UnlinkedUserByCompany} from './pp-read-tail-r40-sources';
import { nativePeopleBatchB } from '@erpx/server-collaboration/adapters/native-people-batch-b';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "@erpx/server-organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { PreparationTask } from "@erpx/feature-people_payroll/lib/people-lifecycle";
export function calendarCommandsPeoplePayrollFacts(db: SqlDatabase, actor?: WorkContext) {
    return { read5: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare(`SELECT t.*,coalesce(e.branch_id,p.branch_id) branch_id,coalesce(e.department_id,p.department_id) department_id FROM people_preparation_tasks t LEFT JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id LEFT JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id LEFT ${organizationFinalSources.calendar_commands_facts_organization_positions_p_join} ON p.id=t.position_id AND p.company_id=t.company_id WHERE t.id=? AND t.company_id=?`).bind(facts.value1, facts.value2).first<PreparationTask & {
            id: string;
            kind: string;
            status: string;
            version: number;
            privileged: boolean;
            assignee_id: string | null;
            branch_id: string | null;
            department_id: string | null;
        }>(),
        read7: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare(ppRd36TwoPredTail("id", "hr_people", "company_id", "user_id", " AND merged_into_id IS NULL")).bind(facts.value1, facts.value2).first<{
            id: string;
        }>(),
        write8: (facts: {
            value1: string | null;
            value2: string | null;
            value3: string;
            value4: number;
            value5: string;
            value6: string;
        }) => nativePeopleBatchB(db).prepReschedule(facts.value6,[facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6]).then(s=>s.run()),
        write9: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
            value5: number;
            value6: string;
            value7: number;
        }) => {
            if (!actor) throw new Error('CALENDAR_NOTIFICATION_ACTOR_REQUIRED');
            return nativeCalendarRetirementNotification(db, actor, facts);
        },
        write11: (facts: {
            value1: "completed" | "not_required";
            value2: string;
            value3: string;
            value4: string;
            value5: number;
            value6: string;
            value7: string;
        }) => nativePeopleBatchB(db).prepComplete(facts.value7,[facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value5, facts.value6, facts.value7]).then(s=>s.run()),
        read25: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare(ppRd32TwoColProjection('assignee_id,responsibility', 'people_preparation_tasks', 'id', 'company_id')).bind(facts.value1, facts.value2).first<{
            assignee_id: string | null;
            responsibility: string;
        }>(),
        read27: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare(ppRd40UnlinkedUserByCompany('id')).bind(facts.value1, facts.value2).first<{
            id: string;
        }>() };
}
