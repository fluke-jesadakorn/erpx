'use client';
import {useId,useState} from 'react';
import {ChevronDown,type LucideIcon} from 'lucide-react';
import {Button} from '@erpx/ui/components/foundation';
import Link from '@erpx/workplace/components/workspace-link';
import styles from '@erpx/feature-pms/components/standard-pms-navigation.module.css';
import ownerStyles from '@erpx/feature-pms/components/standard-pms-backoffice-workspace.module.css';

/** The owner supplies its original permission-filtered items and exact scoped
 * hrefs. This presenter owns only the standard mobile picker and navigation. */
export function StandardPmsBackofficeNavigation({items,current,label}:{
 items:{id:string;label:string;description:string;icon:LucideIcon;href:string}[];
 current:string;label:string;
}){
 const [expanded,setExpanded]=useState(false),menuId=useId();
 const selected=items.find(item=>item.id===current);
 return <div className={`${styles.navigation} ${ownerStyles.navigation}`} data-expanded={expanded}>
  <Button appearance="standard" variant="secondary" type="button" className={styles.picker} aria-expanded={expanded} aria-controls={menuId} onClick={()=>setExpanded(value=>!value)}><span><small>{label}</small><strong>{selected?.label??label}</strong></span><ChevronDown aria-hidden="true" size={18}/></Button>
  <nav id={menuId} className={styles.sections} aria-label={label}>{items.map(({id,label:title,description,icon:Icon,href})=><Link key={id} href={href} className={styles.section} aria-label={title} aria-current={current===id?'page':undefined} onClick={()=>setExpanded(false)}><Icon size={22} aria-hidden="true"/><span><strong>{title}</strong><small>{description}</small></span></Link>)}</nav>
 </div>;
}
