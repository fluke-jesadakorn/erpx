export const payrollPostingPrerequisitesLanding='people-organization/people/payroll/settings/posting';
/** Pure exact presentation selector; native owner source proof is mandatory. */
export function payrollPostingPrerequisitesEntry(destination:string){return destination===payrollPostingPrerequisitesLanding;}
export function payrollPostingPrerequisitesPresentationAllowed(context:{scopeKind:string;beta?:boolean;modules:readonly string[];permissions?:readonly string[]}){return !context.beta&&context.scopeKind==='holding'&&context.modules.includes('finance')&&context.permissions?.includes('finance.approve')===true;}
