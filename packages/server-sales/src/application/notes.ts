import type {Row} from '@erpx/server-commerce/public/commerce';import type {Clock} from '@erpx/server-shared/index';import {BusinessRuleError} from '@erpx/server-shared/index';
import {type ContactNote,type CustomerNotesView} from '@erpx/server-sales/domain/notes';
/** Retained customer note read projections. Commands are native-owned. */
export interface CustomerNoteRepository {
 order(id:string):Promise<Row>;customer(id:string,version?:number):Promise<CustomerNotesView['customer']|null>;
 notes(source:'order'|'customer',id:string):Promise<ContactNote[]>;history(source:'order'|'customer',id:string):Promise<Row[]>;
}
export function createCustomerNoteApplication(ports:{canSell:boolean;canManage:boolean;branchId:string|null;departmentId:string|null;authorize(permission:'commerce.read'|'commerce.sales'):void;isSamePerson(id:Row[string]):boolean;clock:Clock;repository():Promise<CustomerNoteRepository>}){
 const canChange=(note:Row)=>ports.canSell&&(ports.isSamePerson(note.created_by)||ports.canManage);
 function source(order:Row){if(order.kind==='purchase')throw new BusinessRuleError(422,'CUSTOMER_SOURCE_REQUIRED','Use a customer sales order or quotation.');}
 return {
  async forOrder(id:string){ports.authorize('commerce.read');const repository=await ports.repository(),record=await repository.order(id);source(record);const notes=await repository.notes('order',id),history=await repository.history('order',id);
   return {customer:{id:String(record.party_id),name:String(record.party_name),code:String(record.party_code??''),version:Number(record.party_version??1)},order:{id:String(record.id),party_id:String(record.party_id),party_name:String(record.party_name),number:String(record.number),version:Number(record.version)},notes:notes.map(n=>({...n,canChange:n.status==='active'&&canChange(n)})),history,canCreate:ports.canSell} satisfies CustomerNotesView;
  },
  async forCustomer(id:string):Promise<CustomerNotesView>{ports.authorize('commerce.read');const repository=await ports.repository();const customer=await repository.customer(id);if(!customer)throw new BusinessRuleError(404,'CUSTOMER_NOT_FOUND','Customer is not available in this workspace.');const notes=await repository.notes('customer',id),history=await repository.history('customer',id);return {customer,notes:notes.map(n=>({...n,canChange:n.status==='active'&&canChange(n)})),history,canCreate:ports.canSell};},
 };
}
