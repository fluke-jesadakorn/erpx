import { finalRootOwnerProjectionSources as financeProjectionSources } from "../../finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/final-root-owner-projection-sources";
import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {Row} from "../../commerce/public/commerce";
import {holdingResources} from "../../runtime/public/runtime";
import {requirePermission} from "../../identity_access/public/iam-permissions";
import {hasPermission} from "@/lib/permissions";
import {assertion,executeCommand,replayCommand} from "../../workflow/public/commands";
import {ApiError} from "../../runtime/public/http";
import {prepareCustomerIdentityNotice1,prepareCustomerIdentityNotice2} from "../../collaboration/public/customer-authority-notifications";
import {prepareCustomerAuthorityIdentityChange,customerAuthorityIdentitySource,customerAuthorityCandidates} from "../../finance/public/customer-authority-changes";





import {createCustomerIdentityApplication,type IdentityRepository} from "../application/identity";
const manager=`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) a JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=c.company_id AND a.user_id=? AND a.scope_kind='holding' AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.approve') AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
function ports(a:WorkContext) { return {actorId:a.userId,companyScope:!a.branchId&&!a.departmentId,require:(permission:"commerce.read"|"commerce.sales"|"commerce.approve")=>requirePermission(a,permission),canApprove:()=>hasPermission(a,'commerce.approve'),canSales:()=>hasPermission(a,'commerce.sales'),canRead:()=>hasPermission(a,'commerce.read'),clock:{now:()=>Date.now()},error:(status:number,code:string,message:string)=>new ApiError(status,code,message)}; }
function repository(a:WorkContext,write:SqlDatabase):IdentityRepository{return {
 rows:async()=>(await write.prepare(`SELECT c.*,p.name customer_name,p.code customer_code,p.version current_customer_version,CASE WHEN ${manager} THEN 1 ELSE 0 END actor_eligible FROM customer_identity_changes c JOIN (${financeProjectionSources.finance_parties}) p ON p.id=c.customer_id AND p.company_id=c.company_id WHERE c.company_id=? ORDER BY c.updated_at DESC,c.id`).bind(a.userId,a.companyId).all<Row>()).results,
 customers:()=>customerAuthorityCandidates(write,a.companyId,'identity'),history:async id=>(await write.prepare('SELECT * FROM customer_identity_change_events WHERE change_id=? ORDER BY version').bind(id).all<Row>()).results,
 customer:id=>customerAuthorityIdentitySource(write,a.companyId,id),replay:({key,operation,id,input})=>replayCommand(write,a,key,operation,id,input),
 request:async(command,previous,now)=>{const {key,operation:op,id,input}=command;return executeCommand(write,a,key,op,input,id,1,{id,version:1,status:'pending'},[
 assertion(write,key,'customer-version',`EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active' AND version=?)`,[input.customerId,a.companyId,input.expectedCustomerVersion]),
 write.prepare('INSERT INTO customer_identity_changes(id,company_id,customer_id,expected_customer_version,previous_json,proposed_json,evidence,created_by,created_at,updated_at) VALUES(?,?,?,?,?::jsonb,?::jsonb,?,?,?,?)').bind(id,a.companyId,input.customerId,input.expectedCustomerVersion,JSON.stringify(previous),JSON.stringify(input.proposed),input.evidence,a.userId,now,now),
 write.prepare("INSERT INTO customer_identity_change_events VALUES(?,?,1,'request',?,?,?,?::jsonb,?::jsonb,?)").bind(`customer-identity-event-${key}`,id,a.userId,input.evidence,input.expectedCustomerVersion,JSON.stringify(previous),JSON.stringify(input.proposed),now),
 prepareCustomerIdentityNotice1(write,key,id)
 ]);},
 change:async(command,row,facts)=>{const {key,operation:op,id,input}=command,{proposed,customerVersion,status,now}=facts;return executeCommand(write,a,key,op,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1,status},[
 assertion(write,key,'change',`EXISTS(SELECT 1 FROM customer_identity_changes c WHERE c.id=? AND c.company_id=? AND c.version=? AND c.status=? ${input.action==='resubmit'?'':`AND ${manager}`})`,[id,a.companyId,input.expectedVersion,input.action==='resubmit'?'rejected':'pending',...(input.action==='resubmit'?[]:[a.userId])]),
 assertion(write,key,'customer-version',`EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active' AND version=?)`,[String(row.customer_id),a.companyId,customerVersion]),
 ...(input.action==='apply'?[prepareCustomerAuthorityIdentityChange(write,String(row.customer_id),proposed,now)]:[]),
 write.prepare('UPDATE customer_identity_changes SET status=?,expected_customer_version=?,proposed_json=?::jsonb,reviewed_by=?,version=version+1,updated_at=? WHERE id=?').bind(status,customerVersion,JSON.stringify(proposed),input.action==='resubmit'?null:a.userId,now,id),
 write.prepare('INSERT INTO customer_identity_change_events VALUES(?,?,?,?,?,?,?,?,?,?)').bind(`customer-identity-event-${key}`,id,input.expectedVersion+1,input.action,a.userId,input.reason,customerVersion,String(row.previous_json),JSON.stringify(proposed),now),
 prepareCustomerIdentityNotice2(write,key,id)
 ]);},
};}
function application(a:WorkContext){return createCustomerIdentityApplication({...ports(a),repository:async()=>repository(a,(await holdingResources(a.holdingId)).write)});}
export function customerIdentityWorkspace(a:WorkContext){return application(a).customerIdentityWorkspace();}
export function customerIdentityDetail(a:WorkContext,id:string){return application(a).customerIdentityDetail(id);}
export function requestCustomerIdentityChange(a:WorkContext,raw:unknown,key:string){return application(a).requestCustomerIdentityChange(raw,key);}
export function changeCustomerIdentity(a:WorkContext,id:string,raw:unknown,key:string){return application(a).changeCustomerIdentity(id,raw,key);}
export function customerIdentityWork(a:WorkContext){return application(a).customerIdentityWork();}
