import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/finance-server/public/final-root-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {Row} from '@erpx/commerce-server/public/commerce';
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {customerAuthorityCandidates} from "@erpx/finance-server/public/customer-authority-changes";
import {createCustomerDuplicateApplication,type DuplicateRepository} from "@erpx/customer-authority-server/application/duplicates";
import {CA_RD_DUPLICATE_REVIEW_HISTORY,nativeOwnerReadAll} from './native-owner-read';
import {managerEligibilitySql} from './manager-eligibility';
import {reviewEligibilityRows} from './customer-authority-read-tail-r29-sources';
/**
 * Retained duplicate review read projections only. The review/merge commands
 * are native-owned (`CustomerDuplicateHttpCommand::Change`) and are consumed by
 * the duplicate route through `nativeCustomerDuplicateCommand`.
 */
const managerEligible=managerEligibilitySql('r');
function ports(a:WorkContext) { return {actorId:a.userId,companyScope:!a.branchId&&!a.departmentId,require:(permission:"commerce.read"|"commerce.sales"|"commerce.approve")=>requirePermission(a,permission),canApprove:()=>hasPermission(a,'commerce.approve'),canSales:()=>hasPermission(a,'commerce.sales'),canRead:()=>hasPermission(a,'commerce.read'),clock:{now:()=>Date.now()},error:(status:number,code:string,message:string)=>new ApiError(status,code,message)}; }
function repository(a:WorkContext,write:SqlDatabase):DuplicateRepository{return {
 rows:async()=>(await write.prepare(reviewEligibilityRows('r','customer_duplicate_reviews','l.name left_name,l.code left_code,l.tax_registration left_tax,l.status left_status,l.merged_into_id left_merged_into_id,q.name right_name,q.code right_code,q.tax_registration right_tax,q.status right_status,q.merged_into_id right_merged_into_id',`JOIN (${financeProjectionSources.finance_parties}) l ON l.id=r.left_customer_id JOIN (${financeProjectionSources.finance_parties}) q ON q.id=r.right_customer_id`,managerEligible)).bind(a.userId,a.companyId).all<Row>()).results,
 customers:()=>customerAuthorityCandidates(write,a.companyId,'duplicates'),history:async id=>(await nativeOwnerReadAll<Row>(CA_RD_DUPLICATE_REVIEW_HISTORY,[id],write,a.holdingSlug)).results,
 };}
function application(a:WorkContext){return createCustomerDuplicateApplication({...ports(a),repository:async()=>repository(a,(await holdingResources(a.holdingId)).write)});}
export function customerDuplicateWorkspace(a:WorkContext){return application(a).customerDuplicateWorkspace();}
export function customerDuplicateDetail(a:WorkContext,id:string){return application(a).customerDuplicateDetail(id);}
export function customerDuplicateWork(a:WorkContext){return application(a).customerDuplicateWork();}
