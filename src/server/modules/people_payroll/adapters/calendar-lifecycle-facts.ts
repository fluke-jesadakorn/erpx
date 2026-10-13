import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "../../organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {peoplePayrollFinalOwnerProjections as identityaccessFinalOwnerProjections} from "../../identity_access/public/people-payroll-final-owner-projections";
const identityaccessFinalSources=identityaccessFinalOwnerProjections();
import type { ClearanceKind } from "@/lib/people_payroll/retirement";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarLifecyclePeoplePayrollFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: boolean;
            value5: string;
            value6: string;
            value7: string | null;
            value8: boolean;
            value9: string;
            value10: number;
        }) => {
            return db.prepare(`SELECT t.*,iam_same_person(t.assignee_id,?,t.company_id) own_assignee,c.person_id,h.legal_name person_name,coalesce(e.branch_id,p.branch_id) branch_id,coalesce(e.department_id,p.department_id) department_id,assignee.legal_name assignee_name
 FROM people_preparation_tasks t LEFT JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id LEFT JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=t.company_id LEFT JOIN hr_people h ON h.id=c.person_id AND h.company_id=t.company_id LEFT ${organizationFinalSources.calendar_lifecycle_facts_organization_positions_p_join} ON p.id=t.position_id AND p.company_id=t.company_id LEFT JOIN hr_people assignee ON assignee.user_id=t.assignee_id AND assignee.company_id=t.company_id AND assignee.merged_into_id IS NULL
 WHERE t.company_id=? AND ${inList("coalesce(e.department_id,p.department_id,e.branch_id,p.branch_id,t.company_id)")} AND (? OR iam_same_person(t.assignee_id,?,t.company_id))
 AND (t.due_date>=? AND t.due_date<? OR t.status='pending' AND (t.due_date IS NULL OR t.due_date<?))
 AND (?::text IS NULL OR c.person_id=? OR (t.assignee_id=? AND ?))
 AND (?='' OR t.title ILIKE '%'||?||'%' OR h.legal_name ILIKE '%'||?||'%') ORDER BY t.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value1, facts.value5, facts.value6, facts.value6, facts.value7, facts.value7, facts.value1, facts.value8, facts.value9, facts.value9, facts.value9, facts.value10).all<{
                id: string;
                title: string;
                kind: string;
                own_assignee: boolean;
                clearance_type: ClearanceKind | null;
                status: string;
                version: number;
                due_date: string | null;
                responsibility: string;
                privileged: boolean;
                assignee_id: string | null;
                assignee_name: string | null;
                completed_by: string | null;
                completed_at: number | null;
                case_id: string | null;
                person_id: string | null;
                person_name: string | null;
                branch_id: string | null;
                department_id: string | null;
            }>().then(result => result.results);
        },
        read2: (facts: {
            value1: string;
            value2: string;
            value3: number;
            value4: number;
            value5: string;
            value6: string;
            value7: string | null;
            value8: string;
            value9: number;
        }) => {
            return db.prepare(`SELECT c.id,c.kind,c.status,c.version,c.departure_reason,coalesce(c.definition->>'timezone','Asia/Bangkok') timezone,c.effective_at,c.definition->>'lastWorkingDate' last_day,c.person_id,p.legal_name person_name,e.branch_id,e.department_id,x.effective_at cutoff_at,x.restored_at,d.directory_at
 FROM people_lifecycle_cases c JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id LEFT JOIN people_access_cutoffs x ON x.case_id=c.id AND x.company_id=c.company_id AND x.cancelled_at IS NULL
 LEFT JOIN LATERAL (SELECT max(o.updated_at) directory_at ${identityaccessFinalSources.calendar_lifecycle_facts_people_directory_operations_o_from} WHERE o.company_id=c.company_id AND o.status='completed' AND o.result->>'verified'='true' AND o.definition->'action'->>'caseId'=c.id AND o.definition->'action'->>'kind'='account.disable') d ON true
 WHERE c.company_id=? AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")} AND c.status!='cancelled'
 AND (c.effective_at>=? AND c.effective_at<? OR c.definition->>'lastWorkingDate'>=? AND c.definition->>'lastWorkingDate'<? OR x.effective_at>=? AND x.effective_at<? OR d.directory_at>=? AND d.directory_at<?)
 AND (?::text IS NULL OR c.person_id=?) AND (?='' OR p.legal_name ILIKE '%'||?||'%') ORDER BY c.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value3, facts.value4, facts.value3, facts.value4, facts.value7, facts.value7, facts.value8, facts.value8, facts.value9).all<{
                id: string;
                kind: string;
                status: string;
                version: number;
                effective_at: number;
                last_day: string | null;
                departure_reason: string;
                timezone: string;
                person_id: string;
                person_name: string;
                branch_id: string | null;
                department_id: string | null;
                cutoff_at: number | null;
                directory_at: number | null;
                restored_at: number | null;
            }>().then(result => result.results);
        } };
}
