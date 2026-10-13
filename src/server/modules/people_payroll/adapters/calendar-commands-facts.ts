import {calendarRetirementNotification} from "../../collaboration/public/calendar-retirement-notifications";
import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "../../organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { PreparationTask } from "@/lib/people_payroll/people-lifecycle";
export function calendarCommandsPeoplePayrollFacts(db: SqlDatabase) {
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
        }) => db.prepare("SELECT id FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL").bind(facts.value1, facts.value2).first<{
            id: string;
        }>(),
        write8: (facts: {
            value1: string | null;
            value2: string | null;
            value3: string;
            value4: number;
            value5: string;
            value6: string;
        }) => db.prepare("UPDATE people_preparation_tasks SET due_date=?,assignee_id=?,responsibility=?,version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6).run(),
        write9: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
            value5: number;
            value6: string;
            value7: number;
        }) => calendarRetirementNotification(db,facts),
        write11: (facts: {
            value1: "completed" | "not_required";
            value2: string;
            value3: string;
            value4: string;
            value5: number;
            value6: string;
            value7: string;
        }) => db.prepare("UPDATE people_preparation_tasks SET status=?,completion_reason=?,document_references=?::jsonb,completed_by=?,completed_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value5, facts.value6, facts.value7).run(),
        read25: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT assignee_id,responsibility FROM people_preparation_tasks WHERE id=? AND company_id=?").bind(facts.value1, facts.value2).first<{
            assignee_id: string | null;
            responsibility: string;
        }>(),
        read27: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT id FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL LIMIT 1").bind(facts.value1, facts.value2).first<{
            id: string;
        }>() };
}
