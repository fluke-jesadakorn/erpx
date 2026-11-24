import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {customerAuthorityCandidates} from "@erpx/server-finance/public/customer-authority-changes";

import {createCustomerIdentityApplication,type IdentityRepository} from "@erpx/server-customer_authority/application/identity";
import {CA_RD_IDENTITY_CHANGE_HISTORY,nativeOwnerReadAll} from './native-owner-read';
import {managerEligibilitySql} from './manager-eligibility';
/**
 * Retained identity-change read projections only. The request and
 * approve/reject/resubmit commands are native-owned
 * (`CustomerIdentityHttpCommand::Request|Change`) and the feature route consumes
 * only the native client; no TypeScript command fallback remains.
 */
const manager=managerEligibilitySql('c');
function ports(a:WorkContext) { return {actorId:a.userId,companyScope:!a.branchId&&!a.departmentId,require:(permission:"commerce.read"|"commerce.sales"|"commerce.approve")=>requirePermission(a,permission),canApprove:()=>hasPermission(a,'commerce.approve'),canSales:()=>hasPermission(a,'commerce.sales'),canRead:()=>hasPermission(a,'commerce.read'),clock:{now:()=>Date.now()},error:(status:number,code:string,message:string)=>new ApiError(status,code,message)}; }
function repository(a:WorkContext,write:import("@erpx/server-runtime/public/runtime/types").SqlDatabase):IdentityRepository{return {
 rows:async()=>(await write.prepare(`SELECT c.*,p.name customer_name,p.code customer_code,p.version current_customer_version,CASE WHEN ${manager} THEN 1 ELSE 0 END actor_eligible FROM customer_identity_changes c JOIN (${financeProjectionSources.finance_parties}) p ON p.id=c.customer_id AND p.company_id=c.company_id WHERE c.company_id=? ORDER BY c.updated_at DESC,c.id`).bind(a.userId,a.companyId).all<Row>()).results,
 customers:()=>customerAuthorityCandidates(write,a.companyId,'identity'),history:async id=>(await nativeOwnerReadAll<Row>(CA_RD_IDENTITY_CHANGE_HISTORY,[id],write,a.holdingSlug)).results,
 };}
function application(a:WorkContext){return createCustomerIdentityApplication({...ports(a),repository:async()=>repository(a,(await holdingResources(a.holdingId)).write)});}
export function customerIdentityWorkspace(a:WorkContext){return application(a).customerIdentityWorkspace();}
export function customerIdentityDetail(a:WorkContext,id:string){return application(a).customerIdentityDetail(id);}
export function customerIdentityWork(a:WorkContext){return application(a).customerIdentityWork();}
