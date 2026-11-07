'use client';
import {pmsMenu} from '@erpx/feature-shared/components/hospitality/module-menu';
import {pmsSectionPath} from '@erpx/feature-pms/lib/section-route';
import {useFrontLanguage} from '@erpx/front/components/front-language';
import {HospitalityGroupedNavigation} from '@erpx/feature-shared/components/hospitality/grouped-navigation';

/** Daily PMS destinations keep the native owner permission and Property filter. */
export function StandardPmsNavigation({href,section,permissions,propertyId}:{
 href:string;section:string;permissions?:string[];propertyId?:string;
}){
 const language=useFrontLanguage(),can=(permission:string)=>permissions?permissions.includes(permission):true;
 const allowed=[
  ['dashboard',can('pms.read')],
  ['reservations',can('pms.reservations')],
  ['front-desk',can('pms.frontdesk')],
  ['rooms',can('pms.housekeeping')||can('pms.maintenance')],
  ['housekeeping',can('pms.housekeeping')||can('pms.inspect')],
  ['guest-accounts',can('pms.cashier')],
  ['cashier',can('pms.cashier')],
  ['night-audit',can('pms.audit')],
 ] as const;
 const visible=new Set<string>(allowed.filter(([,enabled])=>enabled).map(([id])=>id));
 const items=pmsMenu(language).filter(item=>visible.has(item.id)).map(item=>({
  ...item,href:href+(item.id==='dashboard'?'today':pmsSectionPath(item.id))+(propertyId?'?property='+encodeURIComponent(propertyId):''),
 }));
 return <HospitalityGroupedNavigation app="pms" items={items} current={section} label={language==='th'?'งานโรงแรม':'Hotel tasks'} language={language}/>;
}
