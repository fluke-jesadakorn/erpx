import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "../../organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {OrganizationPerson} from '@/lib/organization';
export async function organizationRosterPeople(db:SqlDatabase,companyId:string){return db.prepare(`SELECT p.id person_id,p.user_id,p.legal_name,p.email,p.version person_version,p.status person_status,e.id engagement_id,e.version engagement_version,
      e.company_id,e.branch_id,e.department_id,e.position_id,e.manager_person_id,e.level_id,l.level_number,l.name level_name,e.status,e.start_date,e.end_date,e.job_title
      FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
      LEFT ${organizationFinalSources.organization_roster_facts_staff_levels_l_join} ON l.id=e.level_id AND l.company_id=e.company_id AND l.active=1
      WHERE p.company_id=? AND p.merged_into_id IS NULL AND p.status!='archived' AND e.status!='ended' ORDER BY p.legal_name,e.id`).bind(companyId).all<OrganizationPerson>();}
export async function organizationRosterExitEngagementIds(db:SqlDatabase,companyId:string,positionId:string){return (await db.prepare("SELECT id FROM hr_engagements WHERE company_id=? AND position_id=? AND status='active'").bind(companyId,positionId).all<{id:string}>()).results.map(x=>x.id);}
export async function organizationMonthlyPayGroups(db:SqlDatabase,companyId:string){return (await db.prepare("SELECT id,name,currency FROM hr_pay_groups WHERE company_id=? AND status='active' AND frequency='monthly' ORDER BY name").bind(companyId).all<{id:string;name:string;currency:string}>()).results;}
