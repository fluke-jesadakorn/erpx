import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {authoritySeatCompanyRead} from './organization-read-tail-sources-r51';
import {personEngagementJoin} from './organization-read-tail-sources-r55';
export function createApprovalsSourceRepository(database:SqlDatabase){return {readCanOverrideStaffStepAppointed(companyId:string,person_id:string,now:number){return database.prepare(authoritySeatCompanyRead('1', "person_id=? AND kind IN ('ceo','hr') AND effective_at<=? LIMIT 1")).bind(companyId,person_id,now).first();},
guard12(key:string,value:string,company_id:string,appointedPersonId:string,userId:string){return assertion(database,key,value,
                  `EXISTS(SELECT 1 FROM authority_seats seat JOIN (${people_payrollPlatformSources.hr_people}) person ON person.id=seat.person_id AND person.company_id=seat.company_id WHERE seat.company_id=? AND seat.kind='hr' AND seat.scope_id=seat.company_id AND seat.person_id=? AND iam_same_person(person.user_id,?,person.company_id) AND person.status='active' AND seat.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`,
                  [company_id,appointedPersonId,userId]);},
guard33(key:string,value:"appointed-approval-current",value2:(string | null)[], policyStep:boolean){return assertion(database,key,value,policyStep
      ? `EXISTS(${personEngagementJoin("WHERE p.id=? AND p.company_id=? AND iam_same_person(p.user_id,?,p.company_id) AND p.status='active' AND e.status='active' AND (?::text IS NULL OR e.position_id=?)")})`
      : `EXISTS(SELECT 1 FROM authority_seats s JOIN (${people_payrollPlatformSources.hr_people}) p ON p.id=s.person_id AND p.company_id=s.company_id WHERE s.company_id=? AND s.kind='hr' AND s.scope_id=s.company_id AND s.person_id=? AND iam_same_person(p.user_id,?,p.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`,
      value2);}};}
