/** Frontend read models and acknowledgement identity; no server-owner imports. */
export type OperationsRow=Record<string,string|number|null>;
export type OperationsSource={kind:string;id:string;version:number;title:string;status:string;companyId:string;branchId:string|null;departmentId:string|null;href:string;label:string};
export type OperationsException={id:string;company_id:string;branch_id:string|null;department_id:string|null;area:string;source_kind:string;source_id:string;source_version:number;source_reference_json:string;policy_id:string;policy_snapshot_json:string;title:string;issue:string;priority:string;status:string;owner_id:string;escalation_owner_id:string|null;resolution:string|null;created_by:string;resolved_by:string|null;reviewed_by:string|null;version:number;created_at:number;updated_at:number;due_at:number;source:OperationsSource};
export type OperationsWorkspace={exceptions:OperationsException[];policies:OperationsRow[];people:OperationsRow[];actorId:string;canWrite:boolean;canReview:boolean;summary:{active:number;awaitingReview:number;escalated:number;overdue:number;byArea:Record<string,number>}};
export type OperationsDetail=OperationsWorkspace&{row:OperationsException;history:OperationsRow[]};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const safe=(v:unknown)=>Number.isSafeInteger(v)&&Number(v)>0;
export function validFieldOperationsWorkspace(v:unknown,company:string):v is OperationsWorkspace{
 if(!object(v)||!Array.isArray(v.exceptions)||!Array.isArray(v.policies)||!Array.isArray(v.people)||typeof v.actorId!=='string'||typeof v.canWrite!=='boolean'||typeof v.canReview!=='boolean'||!object(v.summary))return false;
 if(!['active','awaitingReview','escalated','overdue'].every(k=>Number.isSafeInteger((v.summary as Record<string,unknown>)[k])&&Number((v.summary as Record<string,unknown>)[k])>=0)||!object(v.summary.byArea)||!Object.values(v.summary.byArea).every(x=>Number.isSafeInteger(x)&&Number(x)>=0))return false;
 const row=(v:unknown)=>object(v)&&Object.values(v).every(x=>x===null||typeof x==='string'||typeof x==='number'&&Number.isSafeInteger(x));
 if(!v.policies.every(p=>row(p)&&object(p)&&p.company_id===company&&safe(p.version)&&typeof p.source_kinds_json==='string')||!v.people.every(p=>row(p)&&object(p)&&typeof p.id==='string'&&typeof p.name==='string'))return false;
 return v.exceptions.every(e=>validFieldException(e,company));
}
function validFieldException(e:unknown,company:string):e is OperationsException{
  if(!object(e)||e.company_id!==company||e.source_kind!=='field_service_order'||!safe(e.version)||!safe(e.source_version)||!object(e.source))return false;
  if(!['id','area','source_id','source_reference_json','policy_id','policy_snapshot_json','title','issue','priority','status','owner_id','created_by'].every(k=>typeof e[k]==='string')||!['branch_id','department_id','escalation_owner_id','resolution','resolved_by','reviewed_by'].every(k=>e[k]===null||typeof e[k]==='string')||!Number.isSafeInteger(e.updated_at)||!Number.isSafeInteger(e.due_at))return false;
  const s=e.source;return s.kind==='field_service_order'&&s.companyId===company&&s.id===e.source_id&&safe(s.version)&&['title','status','href'].every(k=>typeof s[k]==='string')&&[s.branchId,s.departmentId].every(x=>x===null||typeof x==='string');
}
export function validFieldOperationsDetailRead(v:unknown,company:string,id:string):v is {row:Omit<OperationsException,'source'>;source:OperationsSource;history:OperationsRow[];canWrite:boolean;canReview:boolean}{
 return object(v)&&object(v.row)&&v.row.id===id&&validFieldException({...v.row,source:v.source},company)&&typeof v.canWrite==='boolean'&&typeof v.canReview==='boolean'&&Array.isArray(v.history)&&v.history.every(row=>object(row)&&row.exception_id===id&&typeof row.id==='string'&&safe(row.version)&&typeof row.action==='string'&&typeof row.reason==='string'&&Object.values(row).every(value=>value===null||typeof value==='string'||typeof value==='number'&&Number.isSafeInteger(value)));
}

/** previousStatus is captured with the attempt, never from a later refresh. */
export function matchesOperationsReceipt(body:string,key:string,value:unknown,previousStatus?:string):boolean{
 if(!object(value))return false;let input:unknown;try{input=JSON.parse(body);}catch{return false;}if(!object(input))return false;
 const policy=input.entity==='policy',update=typeof input.id==='string';let id:unknown,version:number,status:string|undefined;
 if(!update){id=`${policy?'queue-policy':'operations-exception'}-${key}`;version=1;status=policy?'draft':'open';}
 else{if(!safe(input.expectedVersion))return false;id=input.id;version=Number(input.expectedVersion)+1;
 const action=String(input.action);status=policy?({publish:'published',reject:'rejected',retire:'retired'} as Record<string,string>)[action]:({acknowledge:'acknowledged',start:'in_progress',resolve:'awaiting_review',approve:'resolved',reject:'in_progress',close:'closed',reopen:'in_progress',escalate:'escalated',cancel:'cancelled'} as Record<string,string>)[action];
 if(!policy&&action==='assign')status=previousStatus==='awaiting_review'?'in_progress':previousStatus;
 if(!policy&&action==='prioritize')status=previousStatus;
 }
 return Number.isSafeInteger(version)&&value.id===id&&value.version===version&&!!status&&value.status===status;
}
