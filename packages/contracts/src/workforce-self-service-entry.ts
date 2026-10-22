/** Existing personal Work pages retain their People owner. Only the currently
 * registered overview detail section is accepted; no management/task prefix. */
export type WorkforceSelfServiceEntry={kind:'leave'|'overtime';view:'list'|'new'}|{kind:'leave-record'|'overtime-record';id:string};
export type WorkforceSelfServiceEntryScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId?:string};
export function workforceSelfServiceEntry(path:string):WorkforceSelfServiceEntry|null{
 if(path==='leave')return {kind:'leave',view:'list'};
 if(path==='leave/new')return {kind:'leave',view:'new'};
 if(path==='overtime')return {kind:'overtime',view:'list'};
 if(path==='overtime/new')return {kind:'overtime',view:'new'};
 const match=/^(tasks\/people_payroll|overtime)\/([^/]+)(?:\/overview)?$/.exec(path);
 if(!match||!match[2]||/[\\\/?#\u0000-\u001f]/.test(match[2])||['.','..','new'].includes(match[2]))return null;
 return {kind:match[1]==='overtime'?'overtime-record':'leave-record',id:match[2]};
}
