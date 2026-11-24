import { finalRootOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/final-root-owner-projection-sources";
import { employeeRequestSelectHead } from "./read-tail-employee-request-columns";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Canonical owner-local fixed join base for the Records workflow flow-routing
 * source reads. Both callers append their own column list and WHERE predicate to
 * this one admitted join family; no duplicate TS literal remains. The SELECT
 * head is shared through employeeRequestSelectHead; alias/JOIN/tail stay here. */
const flowRoutingRead = (columns: string) =>
  `${employeeRequestSelectHead(columns)} r JOIN (${commerceProjectionSources.commerce_fulfillments}) f ON f.order_id=r.purchase_order_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=f.order_id AND o.company_id=r.company_id WHERE`;
export function createFlowRoutingSourceRepository(database:SqlDatabase){return {readWorkflowRequests(companyId:string){return database.prepare(`${flowRoutingRead('f.id source_id,r.requester_user_id,r.created_by,r.approval_json')} r.company_id=?`).bind(companyId).all<{source_id:string;requester_user_id:string;created_by:string;approval_json:string}>();},
readDefinitionsRequest(source_id:string | number | null,company_id:string){return database.prepare(`${flowRoutingRead('r.requester_user_id,r.created_by,r.approval_json')} f.id=? AND r.company_id=?`).bind(source_id,company_id).first<{requester_user_id:string;created_by:string;approval_json:string}>();}};}
