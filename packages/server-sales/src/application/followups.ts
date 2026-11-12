import type {Row} from '@erpx/server-commerce/public/commerce';
import type {Clock} from '@erpx/server-shared/index';import {BusinessRuleError} from '@erpx/server-shared/index';
import {customerFollowupWorkItems} from '@erpx/server-sales/domain/followups';
/** Retained customer follow-up read projections. Commands are native-owned. */
export interface FollowupRepository {rows(id?:string,orderId?:string):Promise<Row[]>;evidence(id:string,from:string,to:string,beforeDate:string):Promise<Row|null>;recipients(orderId:string):Promise<{id:string;name:string}[]>;history(id:string):Promise<Row[]>;order(id:string):Promise<Row>;}
export function createFollowupApplication(ports:{actorId:string;canRead:boolean;canSell:boolean;canManage:boolean;authorize(permission:'commerce.read'|'commerce.sales'):void;clock:Clock;repository():Promise<FollowupRepository>}){
 async function row(repository:FollowupRepository,id:string){ports.authorize('commerce.read');const result=(await repository.rows(id))[0];if(!result)throw new BusinessRuleError(404,'CUSTOMER_FOLLOWUP_NOT_FOUND','Customer follow-up not found in this scope.');return result;}
 function source(record:Row){if(record.kind==='purchase')throw new BusinessRuleError(422,'CUSTOMER_SOURCE_REQUIRED','Use a customer order or quotation.');}
 return {
  async get(id:string){return row(await ports.repository(),id);},
  async evidence(id:string,from:string,to:string,beforeDate:string){ports.authorize('commerce.read');return (await ports.repository()).evidence(id,from,to,beforeDate);},
  async workspace(orderId?:string){const repository=await ports.repository(),order=orderId?await repository.order(orderId):null;if(order)source(order);ports.authorize('commerce.read');return {order,items:await repository.rows(undefined,orderId),recipients:orderId?await repository.recipients(orderId):[],canCreate:ports.canSell,canManage:ports.canManage,userId:ports.actorId};},
  async detail(id:string){const repository=await ports.repository(),record=await row(repository,id);return {row:record,history:await repository.history(id),recipients:await repository.recipients(String(record.order_id)),canComplete:record.status==='open'&&record.owner_id===ports.actorId&&Number(record.owner_eligible)>0&&ports.canSell,canManage:record.status==='open'&&ports.canSell&&ports.canManage};},
  async work(){if(!ports.canRead)return [];ports.authorize('commerce.read');return customerFollowupWorkItems(await (await ports.repository()).rows(),ports.actorId,ports.canSell);},
 };
}
