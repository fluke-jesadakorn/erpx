export const expensePostingPrerequisitesLanding='finance/setup/expenses';
/** Pure exact destination; current native owner proof is mandatory. */
export function expensePostingPrerequisitesEntry(destination:string){return destination===expensePostingPrerequisitesLanding;}
export function expensePostingPrerequisitesPresentationAllowed(context:{scopeKind:string;beta?:boolean;modules:readonly string[];permissions?:readonly string[]}){return !context.beta&&context.scopeKind==='holding'&&context.modules.includes('finance')&&context.permissions?.includes('finance.read')===true&&['finance.write','finance.approve'].some(permission=>context.permissions?.includes(permission));}
