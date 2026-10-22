export const projectWorkerStartupLanding="projects-routines/projects/settings/workers";
export type ProjectWorkerStartupEntry={personId?:string};
/** Exact destination parser only; the native owner proves current authority. */
export function projectWorkerStartupEntry(destination:string):ProjectWorkerStartupEntry|null{
 if(destination===projectWorkerStartupLanding)return {};
 if(!destination.startsWith(`${projectWorkerStartupLanding}/`))return null;
 let personId:string;try{personId=decodeURIComponent(destination.slice(projectWorkerStartupLanding.length+1));}catch{return null;}
 if(!personId||personId.length>180||[".","..",'new'].includes(personId)||/[\\/?#%\u0000-\u001f\u007f]/.test(personId))return null;
 return {personId};
}
/** Presentation prefilter; never grants a People module or a replacement role. */
export function projectWorkerPresentationAllowed(context:{scopeKind:string;beta?:boolean;permissions?:readonly string[]}){
 return !context.beta&&context.scopeKind==='holding'&&!!context.permissions?.includes('people.manage')&&context.permissions.includes('organization.manage');
}
