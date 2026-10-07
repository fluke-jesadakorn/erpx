import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "../../../organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {peoplePayrollFinalOwnerProjections as identityaccessFinalOwnerProjections} from "../../../identity_access/public/people-payroll-final-owner-projections";
const identityaccessFinalSources=identityaccessFinalOwnerProjections();
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { type PersonRow, type DirectorySelection, type SelectedPersonRow } from '../../domain/directory-projection';
import { createDirectoryProjectionApplication, type DirectoryProjectionRepository } from '../../application/directory-projection';
import { peopleDirectoryAssignmentFacts } from '../../../identity_access/public/directory-projection-facts';
import { peopleDirectoryHierarchyFacts } from '../../../organization/public/directory-projection-facts';
export type { DirectorySelection, DirectorySelectionRecord } from '../../domain/directory-projection';
const currentPeoplePredicate = `p.status='active' AND p.merged_into_id IS NULL
  AND e.status IN ('active','suspended')
  AND e.start_date<=CURRENT_DATE::text
  AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text)`;
const archivedPeopleFrom = `FROM hr_people p
  LEFT JOIN LATERAL (
    SELECT x.id,x.company_id,x.branch_id,x.department_id,x.position_id,x.job_title,x.status
    FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=p.company_id
    ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,
      x.start_date DESC,x.id LIMIT 1
  ) e ON true
  LEFT ${organizationFinalSources.people_directory_repository_organization_positions_op_join}
    ON op.id=e.position_id AND op.company_id=p.company_id AND op.active=1
  WHERE p.company_id=? AND p.merged_into_id IS NULL
    AND (p.status='archived' OR e.status='ended')
    AND NOT EXISTS (SELECT 1 FROM hr_engagements current
      WHERE current.person_id=p.id AND current.company_id=p.company_id
        AND current.status IN ('active','onboarding','suspended'))
    AND (?::text IS NULL OR e.branch_id=?)
    AND (?::text IS NULL OR e.department_id=?)`;
function application(database: SqlDatabase) {
    const repository: DirectoryProjectionRepository = { readDirectoryFacts1(...args) { return peopleDirectoryAssignmentFacts(database).readDirectoryFacts1(...args); },
        readDirectoryFacts2(...args) { return peopleDirectoryHierarchyFacts(database).readDirectoryFacts2(...args); },
        readDirectoryFacts3(...args) { return peopleDirectoryHierarchyFacts(database).readDirectoryFacts3(...args); },
        readDirectoryFacts4(...args) { return peopleDirectoryHierarchyFacts(database).readDirectoryFacts4(...args); },
        readDirectoryFacts5(...args) { return peopleDirectoryHierarchyFacts(database).readDirectoryFacts5(...args); },
        readDirectoryFacts6() {
            return database.prepare(`SELECT p.id AS person_id,COALESCE(p.user_id,(SELECT m.user_id ${identityaccessFinalSources.people_directory_repository_iam_members_m_from} WHERE m.person_id=p.id AND m.company_id=p.company_id AND m.status='active' ORDER BY m.user_id LIMIT 1)) AS user_id,e.id AS engagement_id,e.company_id,e.branch_id,
        e.department_id,e.position_id,p.legal_name,COALESCE(op.title,e.job_title) AS job_title,
        NULLIF(btrim(p.work_email),'') AS work_email,
        NULLIF(btrim(p.work_phone),'') AS work_phone
      FROM hr_engagements e JOIN hr_people p
        ON p.id=e.person_id AND p.company_id=e.company_id
      LEFT ${organizationFinalSources.people_directory_repository_organization_positions_op_join}
        ON op.id=e.position_id AND op.company_id=e.company_id AND op.active=1
      ${organizationFinalSources.people_directory_repository_organization_nodes_c_join}
        ON c.id=e.company_id AND c.kind='company' AND c.status='active'
      WHERE ${currentPeoplePredicate}
      ORDER BY p.legal_name,p.id,e.id`).all<PersonRow>();
        },
        readDirectoryFacts7(value: (string | null)[]) {
            return database.prepare(`SELECT p.id AS person_id,p.user_id,COALESCE(e.id,'') AS engagement_id,
      p.company_id,e.branch_id,e.department_id,e.position_id,p.legal_name,
      COALESCE(op.title,e.job_title,'') AS job_title,
      NULLIF(btrim(p.work_email),'') AS work_email,
      NULLIF(btrim(p.work_phone),'') AS work_phone
    ${archivedPeopleFrom}
    ORDER BY p.legal_name,p.id`).bind(...value).all<PersonRow>();
        },
        readDirectoryFacts8(id: string) {
            return database.prepare(`SELECT p.id AS person_id,e.id AS engagement_id,e.company_id
      FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
      ${organizationFinalSources.people_directory_repository_organization_nodes_c_join} ON c.id=e.company_id AND c.kind='company' AND c.status='active'
      WHERE ${currentPeoplePredicate} AND e.id=?`).bind(id).first<SelectedPersonRow>();
        },
        readDirectoryFacts9(value: (string | null)[], id: string) {
            return database.prepare(`SELECT p.id AS person_id,e.id AS engagement_id,p.company_id
        ${archivedPeopleFrom} AND e.id=?`).bind(...value, id).first<SelectedPersonRow>();
        },
        readDirectoryFacts10(...args) { return peopleDirectoryHierarchyFacts(database).readDirectoryFacts10(...args); },
        readDirectoryFacts11(...args) { return peopleDirectoryHierarchyFacts(database).readDirectoryFacts11(...args); },
        readDirectoryFacts12(...args) { return peopleDirectoryHierarchyFacts(database).readDirectoryFacts12(...args); },
        readDirectoryFacts13(...args) { return peopleDirectoryHierarchyFacts(database).readDirectoryFacts13(...args); } };
    return createDirectoryProjectionApplication({ repository, clock: { now: Date.now } });
}
export async function directoryAssignmentActive(write: SqlDatabase, actor: WorkContext) { return application(write).directoryAssignmentActive(actor); }
export async function readHoldingPeopleDirectory(write: SqlDatabase) { return application(write).readHoldingPeopleDirectory(); }
export async function readArchivedPeopleDirectory(write: SqlDatabase, actor: WorkContext) { return application(write).readArchivedPeopleDirectory(actor); }
export async function readDirectorySelection(write: SqlDatabase, actor: WorkContext, selection: DirectorySelection, includeArchived: boolean) { return application(write).readDirectorySelection(actor, selection, includeArchived); }
