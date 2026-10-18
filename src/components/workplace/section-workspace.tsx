"use client";
import type {ReactNode} from "react";
import { useSearchParams } from "@/lib/shared/destination-search";
import {WorkspaceFrame,WorkspaceNavigation,WorkspaceSections,WorkspaceContent,WorkspaceContext,type WorkspaceSection} from "@/components/ui/workspace";
import {useEmbeddedRecord} from "@/components/records/record-context";
import {WorkspaceRecordHost} from "@/components/records/record-workspace";
import styles from "./section-workspace.module.css";
export function SectionWorkspace({title,items,selected,context,children,toolbar,navigationExtra,embedded=false,nested=false,showHeading=true}:{title:string;items:WorkspaceSection[];selected:string;context?:ReactNode;children:ReactNode;toolbar?:ReactNode;navigationExtra?:ReactNode;embedded?:boolean;nested?:boolean;showHeading?:boolean}) {
 const inRecord=useEmbeddedRecord();
 const query=useSearchParams(),detail=!!query.get('detailKind')&&!!query.get('detailId');
 if(embedded||inRecord)return <div className={styles.embedded}>{inRecord&&<><span hidden data-record-heading>{title}</span><WorkspaceSections items={items} selected={selected} label={`${title} sections`}/></>}{children}</div>;
 return <section className={`${styles.workspace} ${nested?styles.nested:""}`} data-section-workspace onClickCapture={event=>{const link=(event.target as HTMLElement).closest('a');if(!link||event.button!==0||event.metaKey||event.ctrlKey||event.shiftKey||event.altKey||link.target==='_blank')return;if(document.querySelector('[data-record-drawer-content][data-record-dirty="true"]')){event.preventDefault();window.dispatchEvent(new CustomEvent('erpx:record-navigate',{detail:{href:link.href}}));}}}>{showHeading&&<header className={styles.heading}><h1>{title}</h1>{toolbar}</header>}<WorkspaceFrame data-detail-open={detail||undefined}>
 <WorkspaceNavigation title="Views" label={items.find(item=>item.id===selected)?.title} aria-label={`${title} views`}><WorkspaceSections items={items} selected={selected} label={`${title} sections`}/>{navigationExtra}</WorkspaceNavigation>
 <WorkspaceContent>{context&&<WorkspaceContext>{context}</WorkspaceContext>}<div className={styles.content}>{children}</div></WorkspaceContent><WorkspaceRecordHost/>
 </WorkspaceFrame></section>;
}
