import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "@erpx/organization-server/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {activeStaffSelect} from "@erpx/operations-server/public/staff-policy";
import {nativePeoplePayrollReadAll,PEOPLE_PAYROLL_RD_FLOW_ROUTING_PERSON_LEGAL_NAME} from './native-people-payroll-read';
import {paymentBatches,paymentBatchBySource} from './native-lane-9/flow-routing-client';
/** Owner-native flow-routing legal-name reads. SQL is planned by the People Payroll
 * native read port (`RD_FLOW_ROUTING_PERSON_LEGAL_NAME`) and executed on the caller's
 * open unit; this module carries no SQL for the two deduped reads. */
export function createFlowRoutingSourceRepository(database:SqlDatabase){const legalName=(companyId:string,userId:string)=>nativePeoplePayrollReadAll<{legal_name:string}>(PEOPLE_PAYROLL_RD_FLOW_ROUTING_PERSON_LEGAL_NAME,[companyId,userId],database).then(r=>r.results[0]??null);return {/** Native lane 9: the two Company-scoped statements, projection and response
 * shape are owned by the Rust axum/sqlx service; this adapter only transports
 * the bounded rows and fails closed when the native runtime is unavailable. */
readWorkflowPaymentBatches(companyId:string){return paymentBatches(database,companyId);},
readDefinitionsPaymentBatch(company_id:string,id:string | number | null){return paymentBatchBySource(database,company_id,id);},
readReadFlowPeople(companyId:string,now:number,now2:number){return database.prepare(`SELECT s.user_id,s.legal_name,s.department_id,n.display_name team_name FROM (${activeStaffSelect}) s LEFT ${organizationFinalSources.workflow_flow_routing_facts_organization_nodes_n_join} ON n.company_id=s.company_id AND n.id=s.department_id AND n.status='active'`).bind(companyId,now,now2).all<{user_id:string;legal_name:string;department_id:string|null;team_name:string|null}>();},
readPrepareFlowCommandPerson(companyId:string,userId:string){return legalName(companyId,userId);},
readAssignFlowPrimaryAdministrator(companyId:string,userId:string){return legalName(companyId,userId);}};}
