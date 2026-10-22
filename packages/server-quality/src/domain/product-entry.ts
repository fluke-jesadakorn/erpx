/** Canonical existing-source destinations only; never a management collection
 * or new record page. Request labels cannot establish an owner relationship. */
export type QualitySourceEntry={kind:"hold"|"count";id:string};
export type QualityProductPresentationOptions={strictProduct?:true};
export function qualitySourceEntry(path:string):QualitySourceEntry|null{
 const match=/^inventory-quality\/(quality|counts)\/([^/]+)(?:\/overview)?$/.exec(path);
 if(!match||!match[2]||/[\\\/?#\u0000-\u001f]/.test(match[2])||[".","..","new"].includes(match[2]))return null;
 return {kind:match[1]==="quality"?"hold":"count",id:match[2]};
}
export type QualityProductEntryScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string};
