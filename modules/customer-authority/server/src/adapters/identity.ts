import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/finance-server/public/final-root-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {Row} from '@erpx/commerce-server/public/commerce';
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {customerAuthorityCandidates} from "@erpx/finance-server/public/customer-authority-changes";

import {createCustomerIdentityApplication,type IdentityRepository} from "@erpx/customer-authority-server/application/identity";
import {CA_RD_IDENTITY_CHANGE_HISTORY,nativeOwnerReadAll} from './native-owner-read';
import {managerEligibilitySql} from './manager-eligibility';
import {reviewEligibilityRows} from './customer-authority-read-tail-r29-sources';
/**
 * Retained identity-change read projections only. The request and
 * approve/reject/resubmit commands are native-owned
 * (`CustomerIdentityHttpCommand::Request|Change`) and the feature route consumes
 * only the native client; no TypeScript command fallback remains.
 */
const manager=managerEligibilitySql('c');
function ports(a:WorkContext) { return {actorId:a.userId,companyScope:!a.branchId&&!a.departmentId,require:(permission:"commerce.read"|"commerce.sales"|"commerce.approve")=>requirePermission(a,permission),canApprove:()=>hasPermission(a,'commerce.approve'),canSales:()=>hasPermission(a,'commerce.sales'),canRead:()=>hasPermission(a,'commerce.read'),clock:{now:()=>Date.now()},error:(status:number,code:string,message:string)=>new ApiError(status,code,message)}; }
function repository(a:WorkContext,write:import("@erpx/platform-runtime-server/public/runtime/types").SqlDatabase):IdentityRepository{return {
 rows:async()=>(await write.prepare(reviewEligibilityRows('c','customer_identity_changes','p.name customer_name,p.code customer_code,p.version current_customer_version',`JOIN (${financeProjectionSources.finance_parties}) p ON p.id=c.customer_id AND p.company_id=c.company_id`,manager)).bind(a.userId,a.companyId).all<Row>()).results,
 customers:()=>customerAuthorityCandidates(write,a.companyId,'identity'),history:async id=>(await nativeOwnerReadAll<Row>(CA_RD_IDENTITY_CHANGE_HISTORY,[id],write,a.holdingSlug)).results,
 };}
function application(a:WorkContext){return createCustomerIdentityApplication({...ports(a),repository:async()=>repository(a,(await holdingResources(a.holdingId)).write)});}
export function customerIdentityWorkspace(a:WorkContext){return application(a).customerIdentityWorkspace();}
export function customerIdentityDetail(a:WorkContext,id:string){return application(a).customerIdentityDetail(id);}
export function customerIdentityWork(a:WorkContext){return application(a).customerIdentityWork();}
