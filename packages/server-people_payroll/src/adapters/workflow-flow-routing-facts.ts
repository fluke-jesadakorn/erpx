import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "@erpx/server-organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {activeStaffSelect} from "@erpx/server-operations/public/staff-policy";
export function createFlowRoutingSourceRepository(database:SqlDatabase){return {readWorkflowPaymentBatches(companyId:string){return database.prepare("SELECT 1").bind().all<{source_id:string;id:string;status:string;created_by:string}>();},
readDefinitionsPaymentBatch(company_id:string,id:string | number | null){return database.prepare("SELECT 1").bind().first<{id:string;status:string;created_by:string}>();},
readReadFlowPeople(companyId:string,now:number,now2:number){return database.prepare(`SELECT s.user_id,s.legal_name,s.department_id,n.display_name team_name FROM (${activeStaffSelect}) s LEFT ${organizationFinalSources.workflow_flow_routing_facts_organization_nodes_n_join} ON n.company_id=s.company_id AND n.id=s.department_id AND n.status='active'`).bind(companyId,now,now2).all<{user_id:string;legal_name:string;department_id:string|null;team_name:string|null}>();},
readPrepareFlowCommandPerson(companyId:string,userId:string){return database.prepare("SELECT 1").bind().first<{legal_name:string}>();},
readAssignFlowPrimaryAdministrator(companyId:string,userId:string){return database.prepare("SELECT 1").bind().first<{legal_name:string}>();}};}
