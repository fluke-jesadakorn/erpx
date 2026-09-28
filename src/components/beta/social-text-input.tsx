"use client";
import { useEffect, useId, useRef, useState } from "react";
import { activeToken, editMentions, type Mention, type Suggestion } from "@/lib/social-text";
import { betaFetch } from "@/lib/beta-client";
import styles from "./social-text.module.css";
export function SocialTextInput({api,value,mentions,onChange,onPasteImages,onSelectionChange,focusSignal,focusCaret=0,showHint=true,maxLength,label,placeholder,disabled=false}:{api:string;value:string;mentions:Mention[];onChange:(body:string,mentions:Mention[])=>void;onPasteImages?:(files:File[],selection:{start:number;end:number})=>void;onSelectionChange?:(selection:{start:number;end:number})=>void;focusSignal?:number;focusCaret?:number;showHint?:boolean;maxLength:number;label:string;placeholder:string;disabled?:boolean}) {
  const input=useRef<HTMLTextAreaElement>(null), selected=useRef<HTMLButtonElement>(null),listId=useId();
  const [token,setToken]=useState<ReturnType<typeof activeToken>>(null),[results,setResults]=useState<Suggestion[]>([]),[index,setIndex]=useState(0),[status,setStatus]=useState("");
  const [composing,setComposing]=useState(false);
  const trigger=token?.trigger,query=token?.query;
  useEffect(()=>{if(!focusSignal)return;const node=input.current;if(!node)return;node.focus();node.setSelectionRange(focusCaret,focusCaret);},[focusSignal,focusCaret]);
  useEffect(()=>{
    if(!trigger || composing) return;
    let current=true;
    const timer=setTimeout(()=>{
      betaFetch<Suggestion[]>(`${api}/beta/suggestions?${new URLSearchParams({trigger,q:query??""})}`).then(rows=>{if(current){setResults(rows);setIndex(0);setStatus(rows.length?"":trigger==="#"?"Keep typing to create a hashtag.":"No matching people, agents, or records.");}}).catch(()=>{if(current){setResults([]);setStatus("Suggestions unavailable. Keep writing or type @ again to retry.");}});
    },150);
    return ()=>{current=false;clearTimeout(timer);};
  },[api,trigger,query,composing]);
  useEffect(()=>{selected.current?.scrollIntoView({block:"nearest"});},[index,results]);
  function locate(body:string,caret:number) {
    const next=activeToken(body,caret);
    // Never search from inside an already resolved mention.
    const currentMentions=body===value?mentions:editMentions(value,body,mentions);
    const resolved=currentMentions.some(m=>caret>m.start&&caret<=m.end);
    if(next?.query!==token?.query||next?.trigger!==token?.trigger){setResults([]);setStatus("Searching…");setIndex(0);}
    setToken(resolved?null:next);
  }
  function choose(result:Suggestion) {
    if(!token) return;
    const text=`${result.kind==="tag"?"#":"@"}${result.label}`, insert=`${text} `;
    const body=value.slice(0,token.start)+insert+value.slice(token.end);
    if(body.length>maxLength){setStatus("This mention would exceed the message limit.");return;}
    const next=editMentions(value,body,mentions);
    if(result.kind!=="tag")next.push({kind:result.kind,targetId:result.targetId,label:result.label,start:token.start,end:token.start+text.length});
    onChange(body,next.sort((a,b)=>a.start-b.start));setToken(null);
    const caret=token.start+insert.length;
    requestAnimationFrame(()=>{input.current?.focus();input.current?.setSelectionRange(caret,caret);setToken(null);});
  }
  return <div className={styles.editor}>
    <textarea ref={input} value={value} disabled={disabled} aria-label={label} placeholder={placeholder} maxLength={maxLength}
      role="combobox" aria-autocomplete="list" aria-expanded={!!token&&!composing} aria-controls={listId} aria-activedescendant={token&&results[index]?`${listId}-${index}`:undefined}
      onCompositionStart={()=>setComposing(true)} onCompositionEnd={e=>{setComposing(false);locate(e.currentTarget.value,e.currentTarget.selectionStart);}}
      onChange={e=>{const body=e.target.value;onChange(body,editMentions(value,body,mentions));locate(body,e.target.selectionStart);}}
      onPaste={e=>{const images=Array.from(e.clipboardData.items).filter(item=>item.kind==="file"&&item.type.startsWith("image/")).map(item=>item.getAsFile()).filter((file):file is File=>!!file);if(images.length&&onPasteImages){e.preventDefault();onPasteImages(images,{start:e.currentTarget.selectionStart,end:e.currentTarget.selectionEnd});}}}
      onSelect={e=>{locate(e.currentTarget.value,e.currentTarget.selectionStart);onSelectionChange?.({start:e.currentTarget.selectionStart,end:e.currentTarget.selectionEnd});}}
      onKeyDown={e=>{if(!token||composing||e.nativeEvent.isComposing)return;if(e.key==="Escape"){e.preventDefault();setToken(null);}else if(results.length&&["ArrowDown","ArrowUp"].includes(e.key)){e.preventDefault();setIndex(i=>(i+(e.key==="ArrowDown"?1:-1)+results.length)%results.length);}else if(results[index]&&["Enter","Tab"].includes(e.key)){e.preventDefault();choose(results[index]);}}}
      onBlur={e=>{if(!e.currentTarget.parentElement?.contains(e.relatedTarget))setToken(null);}} />
    {token&&!composing&&<div className={styles.suggestions}>
      <div className={styles.caption}>{trigger==="@"?"Mention people, AI agents, or records":"Add a hashtag"}</div>
      <div id={listId} role="listbox" aria-label="Suggestions">{results.map((r,i)=><div key={`${r.kind}:${r.targetId}`}>
        {(i===0||results[i-1].kind!==r.kind)&&<div className={styles.group}>{({person:"People",agent:"AI agents",record:"Records",tag:"Hashtags"})[r.kind]}</div>}
        <button ref={i===index?selected:null} type="button" role="option" id={`${listId}-${i}`} aria-selected={i===index} className={i===index?styles.active:undefined} onMouseDown={e=>e.preventDefault()} onClick={()=>choose(r)}><strong>{r.kind==="tag"?"#":"@"}{r.label}</strong><small>{r.detail}</small></button>
      </div>)}</div>
      <p role="status">{status||"↑ ↓ to choose · Enter to insert · Esc to close"}</p>
    </div>}
    {showHint&&<small className={styles.hint}>Use @ for people, AI agents, and records · # for hashtags{onPasteImages?" · Paste images to attach them":""}</small>}
    {mentions.some(m=>m.kind==="agent")&&<small className={styles.hint}>ERPX assistant will reply after you publish.</small>}
  </div>;
}
