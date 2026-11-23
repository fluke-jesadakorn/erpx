import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "@erpx/server-organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {activeStaffSelect} from "@erpx/server-operations/public/staff-policy";
import {nativePeoplePayrollReadAll,PEOPLE_PAYROLL_RD_FLOW_ROUTING_PERSON_LEGAL_NAME} from './native-people-payroll-read';
/** Owner-native flow-routing legal-name reads. SQL is planned by the People Payroll
 * native read port (`RD_FLOW_ROUTING_PERSON_LEGAL_NAME`) and executed on the caller's
 * open unit; this module carries no SQL for the two deduped reads. */
export function createFlowRoutingSourceRepository(database:SqlDatabase){const legalName=(companyId:string,userId:string)=>nativePeoplePayrollReadAll<{legal_name:string}>(PEOPLE_PAYROLL_RD_FLOW_ROUTING_PERSON_LEGAL_NAME,[companyId,userId],database).then(r=>r.results[0]??null);return {readWorkflowPaymentBatches(companyId:string){return database.prepare("SELECT DISTINCT ON (source_id) source_id,id,status,created_by FROM hr_payment_batches WHERE company_id=? AND source_type='payroll' AND status<>'cancelled' ORDER BY source_id,created_at DESC,id DESC").bind(companyId).all<{source_id:string;id:string;status:string;created_by:string}>();},
readDefinitionsPaymentBatch(company_id:string,id:string | number | null){return database.prepare("SELECT id,status,created_by FROM hr_payment_batches WHERE company_id=? AND source_type='payroll' AND source_id=? AND status<>'cancelled' ORDER BY created_at DESC,id DESC LIMIT 1").bind(company_id,id).first<{id:string;status:string;created_by:string}>();},
readReadFlowPeople(companyId:string,now:number,now2:number){return database.prepare(`SELECT s.user_id,s.legal_name,s.department_id,n.display_name team_name FROM (${activeStaffSelect}) s LEFT ${organizationFinalSources.workflow_flow_routing_facts_organization_nodes_n_join} ON n.company_id=s.company_id AND n.id=s.department_id AND n.status='active'`).bind(companyId,now,now2).all<{user_id:string;legal_name:string;department_id:string|null;team_name:string|null}>();},
readPrepareFlowCommandPerson(companyId:string,userId:string){return legalName(companyId,userId);},
readAssignFlowPrimaryAdministrator(companyId:string,userId:string){return legalName(companyId,userId);}};}
