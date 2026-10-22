export type TalentHireProductEntryScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string};
export type TalentProductPresentationOptions={strictProduct?:true};
export type TalentProductSourceTarget={kind:"hire"|"candidate-privacy";id:string};
export type TalentSourcePurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
/** Only an existing accepted-hire source may be selected across products. */
export function talentHireSourceEntry(path:string):{id:string}|null{
 const match=/^people-organization\/people\/hire-handoffs\/([^/]+)(?:\/overview)?$/.exec(path);
 return match&&match[1]&&!/[%\\\/?#\u0000-\u001f]/.test(match[1])&&![".","..","new","consume"].includes(match[1])?{id:match[1]}:null;
}
/** A privacy action surface is separate from contact-bearing Candidate pages. */
export function talentCandidatePrivacySourceEntry(path:string):{id:string}|null{
 const match=/^people-organization\/talent\/candidates\/([^/]+)\/privacy(?:\/overview)?$/.exec(path);
 return match&&match[1]&&!/[%\\\/?#\u0000-\u001f]/.test(match[1])&&![".","..","new"].includes(match[1])?{id:match[1]}:null;
}
