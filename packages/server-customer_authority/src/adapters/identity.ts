import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {customerAuthorityCandidates} from "@erpx/server-finance/public/customer-authority-changes";

import {createCustomerIdentityApplication,type IdentityRepository} from "@erpx/server-customer_authority/application/identity";
/**
 * Retained identity-change read projections only. The request and
 * approve/reject/resubmit commands are native-owned
 * (`CustomerIdentityHttpCommand::Request|Change`) and the feature route consumes
 * only the native client; no TypeScript command fallback remains.
 */
const manager=`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) a JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=c.company_id AND a.user_id=? AND a.scope_kind='holding' AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.approve') AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
function ports(a:WorkContext) { return {actorId:a.userId,companyScope:!a.branchId&&!a.departmentId,require:(permission:"commerce.read"|"commerce.sales"|"commerce.approve")=>requirePermission(a,permission),canApprove:()=>hasPermission(a,'commerce.approve'),canSales:()=>hasPermission(a,'commerce.sales'),canRead:()=>hasPermission(a,'commerce.read'),clock:{now:()=>Date.now()},error:(status:number,code:string,message:string)=>new ApiError(status,code,message)}; }
function repository(a:WorkContext,write:import("@erpx/server-runtime/public/runtime/types").SqlDatabase):IdentityRepository{return {
 rows:async()=>(await write.prepare(`SELECT c.*,p.name customer_name,p.code customer_code,p.version current_customer_version,CASE WHEN ${manager} THEN 1 ELSE 0 END actor_eligible FROM customer_identity_changes c JOIN (${financeProjectionSources.finance_parties}) p ON p.id=c.customer_id AND p.company_id=c.company_id WHERE c.company_id=? ORDER BY c.updated_at DESC,c.id`).bind(a.userId,a.companyId).all<Row>()).results,
 customers:()=>customerAuthorityCandidates(write,a.companyId,'identity'),history:async id=>(await write.prepare('SELECT * FROM customer_identity_change_events WHERE change_id=? ORDER BY version').bind(id).all<Row>()).results,
 };}
function application(a:WorkContext){return createCustomerIdentityApplication({...ports(a),repository:async()=>repository(a,(await holdingResources(a.holdingId)).write)});}
export function customerIdentityWorkspace(a:WorkContext){return application(a).customerIdentityWorkspace();}
export function customerIdentityDetail(a:WorkContext,id:string){return application(a).customerIdentityDetail(id);}
export function customerIdentityWork(a:WorkContext){return application(a).customerIdentityWork();}
