"use client";
import { useState } from "react";
import Link from "next/link";
import { hashtags, type Mention } from "@/lib/social-text";
import styles from "./social-text.module.css";
export function SocialTextBody({body,mentions=[],base}:{body:string;mentions?:Mention[];base:string}) {
  const [person,setPerson]=useState<Mention|null>(null);
  const spans=[...mentions.map(m=>({...m,type:"mention" as const})),...hashtags(body).filter(t=>!mentions.some(m=>t.start<m.end&&t.end>m.start)).map(t=>({...t,type:"tag" as const}))].sort((a,b)=>a.start-b.start);
  const parts=[];let end=0;
  for(const span of spans){
    if(span.start<end||span.end>body.length)continue;
    parts.push(body.slice(end,span.start));
    const text=body.slice(span.start,span.end);
    if(span.type==="tag")parts.push(<Link key={span.start} className={styles.token} href={`${base}/home?tag=${encodeURIComponent(span.tag)}`}>{text}</Link>);
    else if(span.kind==="record") {const [module,...ids]=span.targetId.split("/");parts.push(<Link key={span.start} className={styles.token} href={`${base}/records/${encodeURIComponent(module)}/${encodeURIComponent(ids.join("/"))}`}>{text}</Link>);}
    else parts.push(<button type="button" key={span.start} className={styles.token} onClick={()=>setPerson(span)}>{text}</button>);
    end=span.end;
  }
  parts.push(body.slice(end));
  return <><p className={styles.body}>{parts}</p>{person&&<div className={styles.identity} role="dialog" aria-label="Mention details"><strong>{person.label}</strong><p>{person.kind==="agent"?"ERPX assistant · AI replies using the conversation and your company’s configured provider.":`Person · ${person.targetId}`}</p><button type="button" onClick={()=>setPerson(null)}>Close</button></div>}</>;
}
