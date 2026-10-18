/** Only actual existing Project task destinations use this included entry.
 * Collections, task creation and query labels cannot establish a source. */
export type ProjectTaskSourceEntry={kind:'task';id:string};
export type ProjectTaskEntryScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string};
export function projectTaskSourceEntry(path:string):ProjectTaskSourceEntry|null{
 const match=/^projects-routines\/tasks\/([^/]+)(?:\/overview)?$/.exec(path);
 if(!match||!match[1]||/[\\\/?#\u0000-\u001f]/.test(match[1])||['.','..','new'].includes(match[1]))return null;
 return {kind:'task',id:match[1]};
}
