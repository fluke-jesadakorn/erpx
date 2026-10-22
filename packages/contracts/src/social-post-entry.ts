/** Only an actual published conversation can use an included source entry.
 * Feed, creation, query labels and generic record mentions prove no source. */
export type SocialPostSourceEntry={kind:'post';id:string};
export type SocialPostEntryScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string};
export function socialPostSourceEntry(path:string):SocialPostSourceEntry|null{
 const match=/^posts\/([^/]+)$/.exec(path);
 if(!match||!match[1]||/[\\\/?#\u0000-\u001f]/.test(match[1])||['.','..','new'].includes(match[1]))return null;
 return {kind:'post',id:match[1]};
}
