import {identitySnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/identity-snapshot-sources';
import {identitySnapshotSources as legalSnapshotSources} from '../../legal/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
const legalSources=legalSnapshotSources();
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {WorkContext} from "../public/authorization";
import type {LegalPeopleQuery,LegalPerson} from "../../legal/public/legal/people-contract";
export async function legalPeopleSource(db:SqlDatabase,a:WorkContext,q:LegalPeopleQuery,scope:{branch:string|null;department:string|null;permission:string|null;itemId:string|null}):Promise<LegalPerson[]>{return (await db.prepare(`SELECT DISTINCT a.user_id id,coalesce(p.legal_name,a.user_id) name,a.role
    FROM assignments a LEFT ${peoplepayrollSources.legal_people_source_hr_people_p_join} ON p.user_id=a.user_id AND p.company_id=a.company_id AND p.merged_into_id IS NULL
    WHERE a.company_id=? AND legal_person_eligible(a.company_id,a.user_id,?,?,?)
    AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id=?)
    AND (?::text IS NULL OR iam_role_allows(a.role,a.company_id,?))
    AND (?::text IS NULL OR a.role=?)
    AND (?='' OR strpos(lower(coalesce(p.legal_name,a.user_id)),lower(?))>0)
    AND (?::text IS NULL OR ?!='reviewer' OR EXISTS(SELECT 1 ${legalSources.legal_people_source_legal_members_m_from} WHERE m.item_id=? AND m.user_id=a.user_id))
    ORDER BY name,id,a.role LIMIT 100`).bind(a.companyId,scope.branch,scope.department,scope.permission,scope.branch,scope.department,scope.permission,scope.permission,q.role??null,q.role??null,q.search,q.search,scope.itemId,q.purpose,scope.itemId).all<{id:string;name:string;role:string}>()).results;}
