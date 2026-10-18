"use client";
import {useId,useState} from "react";
import {ChevronDown,type LucideIcon} from "lucide-react";
import {Button} from "@/components/ui/foundation";
import styles from "./standard-pos-backoffice-console.module.css";

/** Permission filtering and query changes belong to the native owner. */
export function StandardPosBackofficeNavigation({items,current,label,onSelect}:{items:{id:string;label:string;description:string;icon:LucideIcon}[];current:string;label:string;onSelect:(id:string)=>void}) {
  const [expanded,setExpanded]=useState(false),menuId=useId(),selected=items.find(item=>item.id===current);
  return <div className={styles.navigation} data-expanded={expanded}>
    <Button appearance="standard" variant="secondary" type="button" className={styles.picker} aria-expanded={expanded} aria-controls={menuId} onClick={()=>setExpanded(value=>!value)}><span><small>{label}</small><strong>{selected?.label??label}</strong></span><ChevronDown size={18} aria-hidden="true"/></Button>
    <nav id={menuId} className={styles.sections} aria-label={label}>{items.map(({id,label:title,description,icon:Icon})=><Button appearance="standard" variant="plain" type="button" key={id} className={styles.section} aria-label={title} aria-current={current===id?"page":undefined} onClick={()=>{setExpanded(false);onSelect(id);}}><Icon size={22} aria-hidden="true"/><span><strong>{title}</strong><small>{description}</small></span></Button>)}</nav>
  </div>;
}
