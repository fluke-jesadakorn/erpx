import {productAccessLabel} from '@/lib/work_projection/product-access-label';
import 'server-only';
import {adminLaunchChoices} from '@/server/modules/work_projection/public/admin-launcher';
import {productLaunchChoices} from '@/server/modules/work_projection/public/product-launcher';
import {adminWorkspaceTarget} from '@/server/modules/identity_access/public/rules/admin-workspace';
import {product} from '@/server/modules/commercial/public/product-registry';
import {configuredProductHandoff} from '@/lib/runtime/product-hosts';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {OfficeProductPicker} from './office-product-picker';

export async function AdminWorkspaceMenus({companyId,scopeBase,workspaceLabel,management,language}:{companyId:string;scopeBase:string;workspaceLabel:string;management:boolean;language:'en'|'th'}){
 const [adminChoices,productChoices]=await Promise.all([adminLaunchChoices(),productLaunchChoices()]),th=language==='th',kind=management?'management':'company',label=management?'ERPX Management':th?'ดูแลบริษัท':'Administration';
 // Generated links carry public selection hints only. Their destination still
 // runs the normal current Company/role selector before loading owner data.
 const current=(choice:typeof adminChoices[number])=>{
  const next=new URL(choice.href).searchParams.get('next'),target=next?new URL(next,'https://admin.invalid').searchParams.get('next'):null,input=adminWorkspaceTarget(choice.companyId,target);
  return choice.companyId===companyId&&choice.kind===kind&&!!input&&workspaceBase(input.scope)===scopeBase;
 };
 const purpose=(choice:typeof productChoices[number])=>productAccessLabel({purchasePurpose:choice.purchasePurpose,sourceRecord:choice.sourceRecord},language);
 return <OfficeProductPicker productLabel={label} workspaceLabel={workspaceLabel} language={language}
  appsHref={configuredProductHandoff('apps')!} accountHref={configuredProductHandoff('account')!}
  products={[
   ...adminChoices.filter(current).map(choice=>({href:choice.href,label,current:true})),
   ...productChoices.filter(choice=>choice.companyId===companyId&&workspaceBase(choice.scope)===scopeBase).map(choice=>({href:choice.href,label:product(choice.productId)!.label[language],detail:purpose(choice)})),
  ]}
  workspaces={adminChoices.filter(choice=>choice.kind===kind).map(choice=>({href:choice.href,label:`${choice.holding} · ${choice.company}${choice.workspace!==choice.company?` · ${choice.workspace}`:''}`,current:current(choice)}))}/>;
}
