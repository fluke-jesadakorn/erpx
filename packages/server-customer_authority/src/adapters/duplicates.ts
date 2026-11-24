import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {Row} from '@erpx/server-commerce/public/commerce';
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {customerAuthorityCandidates} from "@erpx/server-finance/public/customer-authority-changes";
import {createCustomerDuplicateApplication,type DuplicateRepository} from "@erpx/server-customer_authority/application/duplicates";
import {CA_RD_DUPLICATE_REVIEW_HISTORY,nativeOwnerReadAll} from './native-owner-read';
import {managerEligibilitySql} from './manager-eligibility';
/**
 * Retained duplicate review read projections only. The review/merge commands
 * are native-owned (`CustomerDuplicateHttpCommand::Change`) and are consumed by
 * the duplicate route through `nativeCustomerDuplicateCommand`.
 */
const managerEligible=managerEligibilitySql('r');
function ports(a:WorkContext) { return {actorId:a.userId,companyScope:!a.branchId&&!a.departmentId,require:(permission:"commerce.read"|"commerce.sales"|"commerce.approve")=>requirePermission(a,permission),canApprove:()=>hasPermission(a,'commerce.approve'),canSales:()=>hasPermission(a,'commerce.sales'),canRead:()=>hasPermission(a,'commerce.read'),clock:{now:()=>Date.now()},error:(status:number,code:string,message:string)=>new ApiError(status,code,message)}; }
function repository(a:WorkContext,write:SqlDatabase):DuplicateRepository{return {
 rows:async()=>(await write.prepare(`SELECT r.*,l.name left_name,l.code left_code,l.tax_registration left_tax,l.status left_status,l.merged_into_id left_merged_into_id,q.name right_name,q.code right_code,q.tax_registration right_tax,q.status right_status,q.merged_into_id right_merged_into_id,CASE WHEN ${managerEligible} THEN 1 ELSE 0 END actor_eligible FROM customer_duplicate_reviews r JOIN (${financeProjectionSources.finance_parties}) l ON l.id=r.left_customer_id JOIN (${financeProjectionSources.finance_parties}) q ON q.id=r.right_customer_id WHERE r.company_id=? ORDER BY r.updated_at DESC,r.id`).bind(a.userId,a.companyId).all<Row>()).results,
 customers:()=>customerAuthorityCandidates(write,a.companyId,'duplicates'),history:async id=>(await nativeOwnerReadAll<Row>(CA_RD_DUPLICATE_REVIEW_HISTORY,[id],write,a.holdingSlug)).results,
 };}
function application(a:WorkContext){return createCustomerDuplicateApplication({...ports(a),repository:async()=>repository(a,(await holdingResources(a.holdingId)).write)});}
export function customerDuplicateWorkspace(a:WorkContext){return application(a).customerDuplicateWorkspace();}
export function customerDuplicateDetail(a:WorkContext,id:string){return application(a).customerDuplicateDetail(id);}
export function customerDuplicateWork(a:WorkContext){return application(a).customerDuplicateWork();}
