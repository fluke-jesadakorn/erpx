import type {Clock} from '../../shared';
import {eventWork,projectionWork,type EventMessage,type ProjectionSource,type ProjectionWork} from '../domain/job-projector';
export interface JobProjectorPorts{
 knownHolding(id:string):boolean; clock:Clock; production:boolean;
 pmsEvent(message:EventMessage):Promise<void>;
 customerNotificationId(fulfillmentId:string):Promise<string|null>;
 deliverCustomerNotification(id:string):Promise<void>;
 legalAi():Promise<void>;executiveGeneration():Promise<void>;socialAgent(id:string):Promise<void>;
 expenseAi(id:string):Promise<void>;financeAi():Promise<void>;requestAi(id:string):Promise<void>;
 ownerSource(kind:'task'|'overtime'|'leave'|'payroll',id:string):Promise<ProjectionSource|null>;
 project(message:EventMessage,payload:EventMessage['payload'],version:number,work:ProjectionWork|null,times:{receiptAt:number;workAt:number;checkpointAt:number;updatedAt:number}):Promise<void>;
}
export function createJobProjectorApplication(ports:JobProjectorPorts){
 async function project(message:EventMessage){
  if(!ports.knownHolding(message.holdingId))throw Error('INVALID_HOLDING');
  if(message.eventType.startsWith('pms.'))await ports.pmsEvent(message);
  if(message.eventType==='commerce.customer-notification.queue'||message.eventType==='commerce.customer-notification.retry'){
   const id=message.eventType.endsWith('.queue')?await ports.customerNotificationId(message.aggregateId):message.aggregateId;
   if(id)await ports.deliverCustomerNotification(id);
  }
  if(!ports.production&&message.eventType==='legal.ai.queue')await ports.legalAi();
  if(!ports.production&&message.eventType==='executive.generation.queue')await ports.executiveGeneration();
  if(!ports.production&&['social.post.publish','social.post.comment','social.agent.retry'].includes(message.eventType))await ports.socialAgent(message.aggregateId);
  const flags=eventWork(message);
  if(message.eventType==='expense.ai.prepare')await ports.expenseAi(message.aggregateId);
  if(message.eventType==='finance.ai.prepare')await ports.financeAi();
  if(message.eventType==='request.ai.prepare')await ports.requestAi(message.aggregateId);
  let payload=message.payload,version=message.aggregateVersion;
  if(flags.isTask||((flags.isLeave||flags.isPayroll||flags.isOvertime)&&!payload.state)){
   const owner=await ports.ownerSource(flags.isTask?'task':flags.isOvertime?'overtime':flags.isLeave?'leave':'payroll',message.aggregateId);
   if(!owner)throw Error('PROJECTION_SOURCE_UNAVAILABLE');
   payload={...payload,state:owner.state,version:owner.version,companyId:owner.company_id,branchId:owner.branch_id,departmentId:owner.department_id};version=owner.version;
  }
  const work=projectionWork(flags,payload.state),receiptAt=ports.clock.now(),workAt=work?ports.clock.now():receiptAt;
  await ports.project(message,payload,version,work,{receiptAt,workAt,checkpointAt:ports.clock.now(),updatedAt:ports.clock.now()});
 }
 return {project};
}
