export const posStartupLanding='pos/settings/startup';
export function posStartupEntry(path:string){return path===posStartupLanding||path==='pos/settings';}
/** Menu/entry prefilter only; the native owner proof remains mandatory. */
export function posStartupPresentationAllowed(input:{scopeKind:string;beta?:boolean;permissions?:readonly string[]},stock=true){
 if(input.beta||input.scopeKind==='department')return false;
 const permissions=input.permissions??[];
 return input.scopeKind==='holding'&&permissions.includes('finance.read')&&['finance.write','finance.approve'].some(value=>permissions.includes(value))
  ||stock&&permissions.includes('inventory.read')&&permissions.includes('inventory.prepare');
}
