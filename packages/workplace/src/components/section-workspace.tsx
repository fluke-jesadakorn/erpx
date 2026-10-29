"use client";
import type {ReactNode} from "react";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import {WorkspaceFrame,WorkspaceNavigation,WorkspaceSections,WorkspaceContent,WorkspaceContext,type WorkspaceSection} from "@erpx/ui/components/workspace";
import {useEmbeddedRecord} from "@erpx/ui/components/record-context";
import {WorkspaceRecordHost} from "@erpx/feature-records/components/record-workspace";
import styles from "@erpx/workplace/components/section-workspace.module.css";
export function SectionWorkspace({title,items,selected,context,children,toolbar,navigationExtra,embedded=false,nested=false,showHeading=true}:{title:string;items:WorkspaceSection[];selected:string;context?:ReactNode;children:ReactNode;toolbar?:ReactNode;navigationExtra?:ReactNode;embedded?:boolean;nested?:boolean;showHeading?:boolean}) {
 const inRecord=useEmbeddedRecord();
 const query=useSearchParams(),detail=!!query.get('detailKind')&&!!query.get('detailId');
 if(embedded||inRecord)return <div className={styles.embedded}>{inRecord&&<><span hidden data-record-heading>{title}</span><WorkspaceSections items={items} selected={selected} label={`${title} sections`}/></>}{children}</div>;
 return <section className={`${styles.workspace} ${nested?styles.nested:""}`} data-section-workspace onClickCapture={event=>{const link=(event.target as HTMLElement).closest('a');if(!link||event.button!==0||event.metaKey||event.ctrlKey||event.shiftKey||event.altKey||link.target==='_blank')return;if(document.querySelector('[data-record-drawer-content][data-record-dirty="true"]')){event.preventDefault();window.dispatchEvent(new CustomEvent('erpx:record-navigate',{detail:{href:link.href}}));}}}>{showHeading&&<header className={styles.heading}><h1>{title}</h1>{toolbar}</header>}<WorkspaceFrame style={navigationExtra ? undefined : {gridTemplateColumns:"minmax(0,1fr)"}} data-detail-open={detail||undefined}>
 {navigationExtra && <WorkspaceNavigation title="Organization" aria-label={`${title} organization`}>{navigationExtra}</WorkspaceNavigation>}
 <WorkspaceContent>{context&&<WorkspaceContext>{context}</WorkspaceContext>}<div className={styles.content}>{children}</div></WorkspaceContent><WorkspaceRecordHost/>
 </WorkspaceFrame></section>;
}
