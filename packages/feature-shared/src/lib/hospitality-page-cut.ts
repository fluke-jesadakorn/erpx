import type { FeatureState } from "@erpx/feature-shared/lib/hospitality-rollout";
import {sourcePath,selectionForPath} from '@erpx/feature-work_projection/lib/destination-registry';

export type PageCutEntry = { id:string; name:string; state:FeatureState };

const entryModules:Record<string,string>={
  finance:'finance',expenses:'people_payroll',
  'procurement-rfqs':'procurement','supplier-bills':'procurement','supplier-promises':'procurement',
  'stock-counts':'inventory','stock-transfers':'inventory','stock-transit':'inventory','landed-costs':'inventory',
  'staff-tasks':'operations','routine-templates':'operations',requests:'operations',
  leave:'people_payroll',overtime:'people_payroll',technology:'identity_access',
  assets:'assets',maintenance:'maintenance',quality:'quality',planning:'planning',manufacturing:'manufacturing',
  talent:'talent',learning:'learning',benefits:'benefits',ecommerce:'ecommerce',crm:'crm',support:'support',
  marketing:'marketing',projects:'projects','field-service':'field_service',legal:'legal',agents:'agent',
  executive:'executive',branding:'brand',customers:'customer_360','customer-access':'customer_authority',
  'customer-duplicates':'customer_authority','customer-identity':'customer_authority',
  pos:'pos',pms:'pms',
};

/** Entry screens stay out of a disabled pilot; record detail reads remain available for audit. */
export function productionPageCut(pathname:string,workspaceBase:string,search:Pick<URLSearchParams,'get'>,catalogue:readonly PageCutEntry[]) {
  if(!pathname.startsWith(`${workspaceBase}/`))return null;
  const selection=selectionForPath(pathname),originalSearch=search;
  search={get:key=>selection[key]??originalSearch.get(key)};
  const segments=sourcePath(pathname.slice(workspaceBase.length+1)).split('/').filter(Boolean);
  const [root,section]=segments;
  if(!root)return null;
  if(['sales','customers'].includes(root)&&segments.length===1) {
    const view=search.get('view');
    const owners=root==='sales'
      ? !view||view==='today'?['crm','sales']:['pipeline','reports'].includes(view)?['crm']:['sales']
      : !view?['customer_360','crm','sales','support']:view==='support'?['support']:view==='contacts'?['crm']:view==='reviews'?['customer_authority']:['customer_360','crm','sales'];
    const entries=owners.flatMap(owner=>catalogue.filter(entry=>entry.id===owner));
    if(entries.some(entry=>!['Coming Soon','Prototype'].includes(entry.state)))return null;
    return entries.find(entry=>['Coming Soon','Prototype'].includes(entry.state))??null;
  }
  if(segments.length===1&&(root==='knowledge'&&search.get('view')==='learning'||root==='assets'&&['maintenance','plans'].includes(search.get('view')??'')||root==='settings'&&search.get('section')==='brand')){
    const owner=root==='knowledge'?'learning':root==='assets'?'maintenance':'brand',entry=catalogue.find(item=>item.id===owner);
    return entry&&['Coming Soon','Prototype'].includes(entry.state)?entry:null;
  }
  if(root==='finance'&&segments.length===2&&['engines','group'].includes(section??''))
    return {id:'advanced-finance',name:'Advanced Finance',state:'Coming Soon' as FeatureState};
  // The legacy Payroll entry redirects into its dedicated, read-safe workspace.
  if(root==='people'&&segments.length===1&&search.get('view')==='payroll')return null;
  if(root==='commerce'&&segments.length===1) {
    const view=search.get('view')??'sales';
    const owner=view==='parties'?(search.get('kind')==='supplier'?'procurement':'crm')
      : view==='purchase'?'procurement'
      : ['stock','products','warehouses'].includes(view)?'inventory':'sales';
    if(owner==='sales')return {id:'sales',name:'Sales',state:'Coming Soon' as FeatureState};
    const entry=catalogue.find(item=>item.id===owner);
    return entry&&['Coming Soon','Prototype'].includes(entry.state)?entry:null;
  }
  const peopleSectionOwner=root==='people'&&segments.length===2
    ? section==='organization'?'organization'
      : section==='access'?'identity_access'
        : section==='roster'?'people_payroll'
          : section==='settings'
            ? ['connections','roles','matrix','reviews','groups'].includes(search.get('section')??'')?'identity_access':'people_payroll'
            : null
    : null;
  const moduleId=peopleSectionOwner??(root==='people'&&segments.length===1?'people_payroll':entryModules[root]);
  if(!moduleId)return null;
  const entry=catalogue.find(item=>item.id===moduleId);
  if(!entry||!['Coming Soon','Prototype'].includes(entry.state))return null;
  if(root==='pms') {
    if((section==='documents'&&segments.length>2)||search.get('guestAccount'))return null;
  } else if(root==='pos') {
    if(segments.length>1||search.get('sale')||search.get('movement'))return null;
  } else if(segments.length>1) {
  if(root==='finance'&&segments.length===2)return entry;
    if(root==='people'&&peopleSectionOwner)return entry;
    // Detail pages may contain previously authorized records. Keep those readable.
    if(root!=='support'||!['sla','incidents','sessions'].includes(section??'')||segments.length>2)return null;
  }
  return entry;
}
