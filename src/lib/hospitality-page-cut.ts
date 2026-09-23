import type { FeatureState } from './hospitality-rollout';

export type PageCutEntry = { id:string; name:string; state:FeatureState };

const entryModules:Record<string,string>={
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
  const segments=pathname.slice(workspaceBase.length+1).split('/').filter(Boolean);
  const [root,section]=segments;
  if(!root)return null;
  if(root==='finance'&&segments.length===2&&['engines','group'].includes(section??''))
    return {id:'advanced-finance',name:'Advanced Finance',state:'Coming Soon' as FeatureState};
  if(root==='people'&&segments.length===1&&search.get('view')==='payroll')
    return {id:'statutory-payroll',name:'Payroll processing',state:'Coming Soon' as FeatureState};
  if(root==='commerce'&&segments.length===1&&['sales','quote','pricing'].includes(search.get('view')??''))
    return {id:'sales',name:'Sales',state:'Coming Soon' as FeatureState};
  const moduleId=entryModules[root];
  if(!moduleId)return null;
  const entry=catalogue.find(item=>item.id===moduleId);
  if(!entry||!['Coming Soon','Prototype'].includes(entry.state))return null;
  if(root==='pms') {
    if((section==='documents'&&segments.length>2)||search.get('folio'))return null;
  } else if(root==='pos') {
    if(segments.length>1||search.get('sale')||search.get('movement'))return null;
  } else if(segments.length>1) {
    // Detail pages may contain previously authorized records. Keep those readable.
    if(root!=='support'||!['sla','incidents','sessions'].includes(section??'')||segments.length>2)return null;
  }
  return entry;
}
