"use client";
import { useCallback, useEffect, useRef, useState } from "react";
import { usePathname, useSearchParams } from "next/navigation";
import { BookOpen, MessageSquare, Send, ShieldCheck, Sparkles, X, LoaderCircle } from "lucide-react";
import Link from "next/link";
import { assistantPage, type AssistantView } from "@/lib/ai/workspace-assistant";
import { workFetch } from "@/lib/work-fetch";
import { useHydrated } from "@/components/ui/use-hydrated";
import styles from "./workspace-assistant.module.css";

export function WorkspaceAssistant({base,crossContext}:{base:string;crossContext:boolean}) {
  const pathname=usePathname(),search=useSearchParams();
  const ready=useHydrated();
  const [open,setOpen]=useState(false),[mode,setMode]=useState<"page"|"workspace">("page"),[tab,setTab]=useState<"chat"|"guide">("chat");
  const [contexts,setContexts]=useState<string[]>([]);
  const launcher=useRef<HTMLButtonElement>(null),closeButton=useRef<HTMLButtonElement>(null);
  let path:string|null=null;
  try{path=assistantPage(`${pathname.slice(base.length)}${search.size?`?${search}`:""}`).path;}catch{}
  const close=useCallback(()=>{setOpen(false);requestAnimationFrame(()=>launcher.current?.focus());},[]);
  useEffect(()=>{if(!open)return;closeButton.current?.focus();const handle=(e:KeyboardEvent)=>{if(e.key==="Escape"){e.preventDefault();close();}};window.addEventListener("keydown",handle);return()=>window.removeEventListener("keydown",handle);},[open,close]);
  if(!path)return null;
  const guide=assistantPage(path).guide;
  return <div data-app-chrome>
    {!open&&<button ref={launcher} disabled={!ready} className={styles.launcher} onClick={()=>setOpen(true)} aria-label="Open work assistant"><Sparkles size={19}/><span>Ask ERPX</span></button>}
    {open&&<aside className={styles.panel} aria-label="ERPX work assistant">
      <header className={styles.header}><div className={styles.brand}><Sparkles/><div><strong>Your work companion</strong><small>ERPX Assistant</small></div></div><button ref={closeButton} className={styles.icon} onClick={close} aria-label="Close work assistant"><X size={20}/></button></header>
      <div className={styles.context}><label><ShieldCheck size={14}/>{crossContext?<select aria-label="Assistant context scope" value={mode} onChange={e=>{setMode(e.target.value as typeof mode);setContexts([]);}}><option value="page">This page · {guide.title}</option><option value="workspace">Follow my work · permitted pages</option></select>:<strong>This page · {guide.title}</strong>}</label><p>{mode==="page"?"Separate conversation for this page and record.":"Current page plus the contexts you keep below. Your selected role still applies."}</p>{mode==="workspace"&&<div className={styles.chips}>{contexts.map(p=><button key={p} onClick={()=>setContexts(c=>c.filter(v=>v!==p))} aria-label={`Remove context ${p}`}>{assistantPage(p).guide.title} ×</button>)}{!contexts.includes(path)&&contexts.length<5&&<button onClick={()=>setContexts(c=>[...c,path!])}>+ Keep {guide.title} as context</button>}</div>}</div>
      <div className={styles.tabs}><button aria-pressed={tab==="chat"} onClick={()=>setTab("chat")}><MessageSquare size={15}/>Ask & prepare</button><button aria-pressed={tab==="guide"} onClick={()=>setTab("guide")}><BookOpen size={15}/>Teach me</button></div>
      {tab==="guide"?<div className={styles.body}><div className={styles.empty}><h3>Work confidently in {guide.title}</h3><p>{guide.purpose}</p></div><ol className={styles.steps}>{guide.steps.map(s=><li key={s}>{s}</li>)}</ol><p className={styles.notice}>Workflow guidance is available even when AI is offline. Company rules come from your published handbook.</p><Link href={`${base}/handbook`}>Open company handbook →</Link></div>:<AssistantConversation key={mode==="page"?path:"workspace"} base={base} path={path} mode={mode} contextPaths={contexts}/>}
    </aside>}
  </div>;
}

function AssistantConversation({base,path,mode,contextPaths}:{base:string;path:string;mode:"page"|"workspace";contextPaths:string[]}) {
  const [loadedView,setView]=useState<AssistantView|null>(null),[question,setQuestion]=useState(""),[error,setError]=useState(""),[busy,setBusy]=useState(false),[loading,setLoading]=useState(true);
  const current=useRef(path),generation=useRef(0),mounted=useRef(true),tail=useRef<HTMLDivElement>(null);
  const guide=assistantPage(path).guide;
  const view=loadedView?.page===path?loadedView:null;
  const pending=loading||!view;
  const load=useCallback(async(signal?:AbortSignal)=>{
    const response=await workFetch(`/api/v1${base}/assistant?${new URLSearchParams({path,mode})}`,{cache:"no-store",signal});
    const body=await response.json();if(!response.ok)throw Error(body.error?.message??"Assistant unavailable.");return body.data as AssistantView;
  },[base,path,mode]);
  useEffect(()=>{mounted.current=true;current.current=path;const token=++generation.current,abort=new AbortController();void load(abort.signal).then(v=>{if(token===generation.current){setView(v);setError("");}}).catch(e=>{if(!abort.signal.aborted)setError(e instanceof Error?e.message:"Assistant unavailable.");}).finally(()=>{if(!abort.signal.aborted)setLoading(false);});return()=>{mounted.current=false;abort.abort();};},[path,load]);
  useEffect(()=>{tail.current?.scrollIntoView({block:"nearest"});},[view?.turns.length,busy]);
  async function send(text:string) {
    if(busy||!text.trim())return;
    const sourcePath=path,token=generation.current;
    setBusy(true);setError("");
    try {
      const response=await workFetch(`/api/v1${base}/assistant`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":crypto.randomUUID()},body:JSON.stringify({path:sourcePath,mode,question:text,contextPaths})});
      const body=await response.json();if(!response.ok)throw Error(body.error?.message??"AI could not respond. Retry or continue manually.");
      if(mounted.current&&current.current===sourcePath&&token===generation.current){const updated=await load();if(mounted.current&&current.current===sourcePath&&token===generation.current){setQuestion("");setView(updated);}}
    }catch(e){if(mounted.current&&token===generation.current)setError(e instanceof Error?e.message:"AI is unavailable.");}
    finally{if(mounted.current)setBusy(false);}
  }
  return <><div className={styles.body} aria-busy={busy||pending}>
    {!view?.turns.length&&<div className={styles.empty}><h3>What would help you move forward?</h3><p>Ask about {guide.title.toLowerCase()}, explain a step, or check what to prepare. Record details are included only where a supported, authorized source is available.</p><div className={styles.prompts}>{guide.prompts.map(p=><button key={p} disabled={pending||busy||!!view?.unavailable||!view} onClick={()=>{setQuestion(p);void send(p);}}>{p} →</button>)}</div></div>}
    {!!view?.hiddenTurns&&<p className={styles.notice}>Some earlier answers are hidden because their sources or your access changed.</p>}
    {view?.turns.map(t=><article key={t.id} className={styles.turn}><div className={styles.question}>{t.question}</div><div className={styles.answer}>{t.answer.answer}</div><div className={styles.sources}>{t.sources.filter(s=>t.answer.sourceIds.includes(s.id)).map(s=><Link key={`${s.path}:${s.id}`} href={`${base}${s.path}`}>{s.title}</Link>)}</div></article>)}
    {view?.unavailable&&<p role="status" className={styles.notice}>{view.unavailable}</p>}
    {error&&<p role="alert" className={styles.notice}>{error}</p>}
    {pending&&!error&&<p className={styles.busy}><LoaderCircle className={styles.spin} size={15}/>Loading this conversation…</p>}
    {busy&&<p role="status" className={styles.busy}><LoaderCircle className={styles.spin} size={15}/>Preparing an answer for this context…</p>}<div ref={tail}/>
  </div><form className={styles.composer} onSubmit={e=>{e.preventDefault();void send(question);}}><div className={styles.input}><textarea aria-label="Message to work assistant" placeholder="Ask about this page…" value={question} maxLength={4000} onChange={e=>setQuestion(e.target.value)} rows={2}/><button aria-label="Send to assistant" disabled={busy||pending||!view||!!view.unavailable||!question.trim()}><Send size={17}/></button></div><small>AI helps prepare. You review and submit the work.</small></form></>;
}
