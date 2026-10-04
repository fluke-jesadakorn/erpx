"use client";
import { useEffect,useId,useState,type ReactNode } from "react";
import { useSearchParams } from "next/navigation";
import { useFrontLanguage } from "@/components/front/front-language";
export function RecordSections({children,evidence,history}:{children:ReactNode;evidence:ReactNode;history:ReactNode}){
 const query=useSearchParams(),sections=["overview","evidence","history"];
 const [active,setActive]=useState(Math.max(0,sections.indexOf(query.get("section")??"overview"))),id=useId(),th=useFrontLanguage()==="th",labels=th?["ภาพรวม","หลักฐาน","ประวัติ"]:["Overview","Evidence","History"];
 function select(i:number){setActive(i);const url=new URL(location.href);url.searchParams.set("section",sections[i]);window.history.replaceState(window.history.state,"",url);}
 useEffect(()=>{const select=()=>{if(["#attachments","#evidence"].includes(location.hash))setActive(1);};select();window.addEventListener("hashchange",select);return()=>window.removeEventListener("hashchange",select);},[]);
 return <section><div className="ui-tab-nav" role="tablist" aria-label={th?"ข้อมูลคำขอ":"Request information"}>{labels.map((label,i)=><button className="ui-tab-link" key={label} role="tab" id={`${id}-tab-${i}`} aria-selected={active===i} aria-controls={`${id}-panel-${i}`} tabIndex={active===i?0:-1} onClick={()=>select(i)} onKeyDown={e=>{const next=e.key==="ArrowRight"?(i+1)%3:e.key==="ArrowLeft"?(i+2)%3:e.key==="Home"?0:e.key==="End"?2:null;if(next!==null){e.preventDefault();select(next);(e.currentTarget.parentElement?.children[next] as HTMLButtonElement)?.focus();}}}>{label}</button>)}</div>{[children,evidence,history].map((content,i)=><div key={i} role="tabpanel" id={`${id}-panel-${i}`} aria-labelledby={`${id}-tab-${i}`} hidden={active!==i} tabIndex={0}>{content}</div>)}</section>;
}
