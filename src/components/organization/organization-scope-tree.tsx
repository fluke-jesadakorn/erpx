"use client";
import {useState} from "react";
import { usePathname } from "next/navigation";
import { useSearchParams } from "@/lib/shared/destination-search";
import { useRouter } from "@/lib/shared/destination-router";
import {Button} from "@/components/ui/button";
import {ChevronDown,ChevronRight} from "lucide-react";
import styles from "./organization-structure.module.css";
export type ScopeNode={id:string;name:string;parentId:string|null};
/** Unit-only projection: no employee, position, or private HR data is required. */
export function OrganizationScopeTree({nodes}:{nodes:ScopeNode[]}){
 const query=useSearchParams(),router=useRouter(),path=usePathname(),[expanded,setExpanded]=useState(()=>new Set(nodes.map(node=>node.id))),selected=query.get('unit');
 function select(id:string|null){const next=new URLSearchParams(query);for(const key of ['detailKind','detailId','detailArgs','detailPage','detailMode','cursor'])next.delete(key);if(id)next.set('unit',id);else next.delete('unit');const href=`${path}?${next}`;if(document.querySelector('[data-record-drawer-content][data-record-dirty="true"]'))window.dispatchEvent(new CustomEvent('erpx:record-navigate',{detail:{href}}));else router.push(href,{scroll:false});}
 function render(node:ScopeNode,seen:Set<string>,depth:number):React.ReactNode{if(seen.has(node.id))return null;const visited=new Set([...seen,node.id]),children=nodes.filter(item=>item.parentId===node.id&&item.id!==node.id);return <div key={node.id} className={styles.unitGroup}><div className={styles.unitRow} style={{marginLeft:Math.min(depth,4)*12}} data-selected={selected===node.id||undefined}>{children.length>0&&<Button appearance="standard" variant="plain" type="button" aria-label={`${expanded.has(node.id)?'Collapse':'Expand'} ${node.name}`} aria-expanded={expanded.has(node.id)} onClick={event=>{event.stopPropagation();setExpanded(previous=>{const next=new Set(previous);if(next.has(node.id))next.delete(node.id);else next.add(node.id);return next;});}}>{expanded.has(node.id)?<ChevronDown size={16}/>:<ChevronRight size={16}/>}</Button>}<Button appearance="standard" variant="plain" type="button" className={styles.unitName} aria-pressed={selected===node.id} onClick={()=>select(node.id)}>{node.name}</Button></div>{expanded.has(node.id)&&children.map(child=>render(child,visited,depth+1))}</div>;}
 return <nav aria-label="Organization units"><Button appearance="standard" variant="plain" type="button" aria-pressed={!selected} onClick={()=>select(null)}>All permitted units</Button>{nodes.filter(node=>!node.parentId||!nodes.some(parent=>parent.id===node.parentId)||node.parentId===node.id).map(node=>render(node,new Set(),0))}</nav>;
}
