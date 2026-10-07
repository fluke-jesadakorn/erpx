import type {Row} from '../../commerce/public/commerce';import type {Clock} from '../../shared';import {BusinessRuleError} from '../../shared';
import {customerNoteSchema,customerNoteChangeSchema,customerNoteInScope,type ContactNote,type CustomerNotesView,type CustomerNoteCreate,type CustomerNoteChange} from '../domain/notes';
export type CustomerNoteMutation={key:string;id:string;operation:string;now:number;result:Record<string,unknown>} & ({action:'create-order';input:CustomerNoteCreate;record:Row}|{action:'create-customer';input:CustomerNoteCreate;customerId:string}|{action:'change';input:CustomerNoteChange;record:Row;content:Row[string];status:string});
export interface CustomerNoteRepository {
 order(id:string):Promise<Row>;timeline(customerId:string):Promise<void>;customer(id:string,version?:number):Promise<CustomerNotesView['customer']|null>;
 notes(source:'order'|'customer',id:string):Promise<ContactNote[]>;history(source:'order'|'customer',id:string):Promise<Row[]>;note(id:string):Promise<Row|null>;
 replay(key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;commit(mutation:CustomerNoteMutation):Promise<Record<string,unknown>>;
}
export function createCustomerNoteApplication(ports:{canSell:boolean;canManage:boolean;branchId:string|null;departmentId:string|null;authorize(permission:'commerce.read'|'commerce.sales'):void;isSamePerson(id:Row[string]):boolean;clock:Clock;repository():Promise<CustomerNoteRepository>}){
 const canChange=(note:Row)=>ports.canSell&&(ports.isSamePerson(note.created_by)||ports.canManage);
 function source(order:Row){if(order.kind==='purchase')throw new BusinessRuleError(422,'CUSTOMER_SOURCE_REQUIRED','Use a customer sales order or quotation.');}
 return {
  async forOrder(id:string){ports.authorize('commerce.read');const repository=await ports.repository(),record=await repository.order(id);source(record);const notes=await repository.notes('order',id),history=await repository.history('order',id);
   return {customer:{id:String(record.party_id),name:String(record.party_name),code:String(record.party_code??''),version:Number(record.party_version??1)},order:{id:String(record.id),party_id:String(record.party_id),party_name:String(record.party_name),number:String(record.number),version:Number(record.version)},notes:notes.map(n=>({...n,canChange:n.status==='active'&&canChange(n)})),history,canCreate:ports.canSell} satisfies CustomerNotesView;
  },
  async forCustomer(id:string):Promise<CustomerNotesView>{ports.authorize('commerce.read');const repository=await ports.repository();await repository.timeline(id);const customer=await repository.customer(id);if(!customer)throw new BusinessRuleError(404,'CUSTOMER_NOT_FOUND','Customer is not available in this workspace.');const notes=await repository.notes('customer',id),history=await repository.history('customer',id);return {customer,notes:notes.map(n=>({...n,canChange:n.status==='active'&&canChange(n)})),history,canCreate:ports.canSell};},
  async create(raw:unknown,key:string){const input=customerNoteSchema.parse(raw);ports.authorize('commerce.sales');ports.authorize('commerce.read');if(input.occurredAt>ports.clock.now()+60000)throw new BusinessRuleError(422,'CONTACT_TIME_INVALID','Record a contact that already happened.');
   const repository=await ports.repository(),id=`contact-${key}`,operation='customer.note.create',prior=await repository.replay(key,operation,id,input);if(prior)return prior;const now=ports.clock.now(),result={id,version:1};
   if(input.orderId){const record=await repository.order(input.orderId);source(record);return repository.commit({action:'create-order',input,key,id,operation,now,record,result});}
   const customerId=String(input.customerId);await repository.timeline(customerId);if(!await repository.customer(customerId,input.expectedVersion))throw new BusinessRuleError(409,'CUSTOMER_SOURCE_CHANGED','Refresh the Customer 360 record before recording a note.');return repository.commit({action:'create-customer',input,key,id,operation,now,customerId,result});
  },
  async change(id:string,raw:unknown,key:string){const input=customerNoteChangeSchema.parse(raw);ports.authorize('commerce.sales');ports.authorize('commerce.read');const repository=await ports.repository(),operation=`customer.note.${input.action}`,prior=await repository.replay(key,operation,id,input);if(prior)return prior;
   const record=await repository.note(id);if(!record||!customerNoteInScope(record,ports.branchId,ports.departmentId))throw new BusinessRuleError(404,'CUSTOMER_NOTE_NOT_FOUND','Contact note not found in this scope.');if(record.order_id===null)await repository.timeline(String(record.customer_id));if(!canChange(record))throw new BusinessRuleError(403,'CUSTOMER_NOTE_OWNER_REQUIRED','The author or a sales manager must correct this contact record.');
   const content=input.action==='correct'?input.body:record.body,status=input.action==='archive'?'archived':'active',now=ports.clock.now(),result={id,version:input.expectedVersion+1,status};return repository.commit({action:'change',input,key,id,operation,now,record,content,status,result});
  }
 };
}
