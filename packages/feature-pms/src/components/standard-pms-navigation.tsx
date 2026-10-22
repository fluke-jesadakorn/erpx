'use client';
import {useId,useState} from 'react';
import {ChevronDown} from 'lucide-react';
import {Button} from '@erpx/ui/components/foundation';
import Link from '@erpx/workplace/components/workspace-link';
import {pmsMenu} from '@erpx/feature-shared/components/hospitality/module-menu';
import {pmsSectionPath} from '@erpx/feature-pms/lib/section-route';
import {useFrontLanguage} from '@erpx/front/components/front-language';
import styles from '@erpx/feature-pms/components/standard-pms-navigation.module.css';

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
 const label=language==='th'?'งานโรงแรม':'Hotel tasks',selected=items.find(item=>item.id===section);
 const [expanded,setExpanded]=useState(false),menuId=useId();
 return <div className={styles.navigation} data-expanded={expanded}>
  <Button appearance="standard" variant="secondary" type="button" className={styles.picker} aria-expanded={expanded} aria-controls={menuId} onClick={()=>setExpanded(value=>!value)}><span><small>{label}</small><strong>{selected?.label??label}</strong></span><ChevronDown aria-hidden="true" size={18}/></Button>
  <nav id={menuId} className={styles.sections} aria-label={label}>{items.map(({id,label:title,description,icon:Icon,href})=><Link key={id} href={href} className={styles.section} aria-label={title} aria-current={section===id?'page':undefined} onClick={()=>setExpanded(false)}><Icon size={22} aria-hidden="true"/><span><strong>{title}</strong><small>{description}</small></span></Link>)}</nav>
 </div>;
}
