export type EventMessage={holdingId:string;id:string;eventType:string;aggregateId:string;aggregateVersion:number;payload:{id:string;state?:string;version:number;companyId:string;branchId:string|null;departmentId:string|null}};
export type ProjectionSource={state:string;version:number;company_id:string;branch_id:string|null;department_id:string|null};
export type ProjectionWork={sourceType:string;title:string;queue:'completed'|'todo'|'waiting'};
export function eventWork(message:EventMessage):{isLeave:boolean;isOvertime:boolean;isPayroll:boolean;isTask:boolean}{return {isLeave:message.eventType.startsWith('leave.')&&!message.eventType.startsWith('leave.policy.'),isOvertime:message.eventType.startsWith('overtime.')&&message.eventType!=='overtime.eligibility',isPayroll:message.eventType.startsWith('people.payroll.'),isTask:message.eventType.startsWith('task.')||message.eventType==='routine.run'};}
export function projectionWork(flags:ReturnType<typeof eventWork>,state:string|undefined):ProjectionWork|null{
 if(!flags.isLeave&&!flags.isOvertime&&!flags.isPayroll&&!flags.isTask)return null;
 const completed=['approved','posted','rejected','cancelled','completed','withdrawn'].includes(state??'');
 return {queue:completed?'completed':state==='pending_approval'||state==='preview'?'todo':'waiting',sourceType:flags.isOvertime?'overtime_request':flags.isTask?'staff_task':flags.isPayroll?'hr_payroll_run':'leave_request',title:flags.isOvertime?'Overtime request':flags.isTask?'Staff task':flags.isPayroll?'Payroll preview':'Leave request'};
}
