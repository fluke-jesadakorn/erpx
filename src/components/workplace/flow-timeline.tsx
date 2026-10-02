"use client";

import { workFetch } from "@/lib/work-fetch";
import { useState } from "react";
import { useRouter } from "next/navigation";
import type { FlowView } from "@/lib/flow";
import styles from "./flow.module.css";
import { TimeText } from "@/components/time/time-preference-context";

export function FlowTimeline({flow,endpoint}:{flow:FlowView;endpoint:string}) {
  const router = useRouter();
  const [busy,setBusy] = useState(false), [error,setError] = useState("");
  async function assign(event:React.FormEvent<HTMLFormElement>,stepKey:string) {
    event.preventDefault(); const form = new FormData(event.currentTarget); setBusy(true); setError("");
    try {
      const response = await workFetch(endpoint,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":crypto.randomUUID()},body:JSON.stringify({stepKey,userId:form.get("userId"),reason:form.get("reason"),saveDefault:form.get("saveDefault")==="on",expectedVersion:flow.version,recordVersion:flow.recordVersion})});
      const body = await response.json(); if (!response.ok) throw new Error(body.error?.message ?? "Assignment failed."); router.refresh();
    } catch (cause) { setError((cause as Error).message); } finally {setBusy(false);}
  }
  return <section id="workflow-owners" tabIndex={-1} className={`card ${styles.flow}`} aria-label="Workflow and next responsible staff">
    <header><div><p className="eyebrow">Flow · Teams & owners</p><h2>{flow.current?.action ?? "No pending action"}</h2></div><span className="badge">Flow status: {flow.state.replaceAll("_"," ")}</span></header>
    {flow.current && <p>Now: <strong>{flow.current.primary?.name ?? "No owner assigned"}</strong> · {flow.current.teamName} · {flow.current.role.replaceAll("_"," ")}{flow.canAct && <strong> · Your task</strong>}</p>}
    {flow.next && <p className="muted">Next: {flow.next.action} → {flow.next.teamName} / {flow.next.primary?.name ?? "Waiting for owner"}</p>}
    {flow.blocker && <p role="status" className="notice">{flow.blocker}</p>}
    {error && <p role="alert" className="error">{error}</p>}
    <ol className={styles.steps}>{flow.steps.map((step,index) => <li key={step.key} className={step.status === "current" ? styles.current : undefined} aria-current={step.status === "current" ? "step" : undefined}>
      <span className={styles.number}>{index+1}</span><div><strong>{step.action}</strong><p>{step.teamName} · {step.primary?.name ?? "Unassigned"}</p><small>{step.role.replaceAll("_"," ")} · {step.status === "current" ? "Pending" : step.status === "recorded" ? "Recorded" : "Flow step"}</small>
      {flow.current && step.blocker && step.status !== "recorded" && <p className="muted">{step.blocker}</p>}
      {flow.canManage && flow.current && index >= flow.steps.indexOf(flow.current) && <details><summary>Assign / change owner</summary><form onSubmit={event => void assign(event,step.key)}>
        <label>Owner<select name="userId" required defaultValue={step.primary?.userId ?? ""} disabled={busy}><option value="">Select eligible person</option>{step.candidates.map(p => <option key={p.userId} value={p.userId}>{p.name} · {p.teamName}</option>)}</select></label>
        <label>Reason<input name="reason" required minLength={3} maxLength={2000} disabled={busy}/></label>
        <label className="check-row"><input name="saveDefault" type="checkbox" disabled={busy}/>Use as the default for new documents in this scope</label>
        <button className="button secondary" disabled={busy || !step.candidates.length}>Save owner</button>
      </form></details>}
      </div></li>)}</ol>
    {!!flow.history.length && <details><summary>Handoff history ({flow.history.length})</summary><ul>{flow.history.map((e,index) => <li key={index}>{e.actorName} · {e.action} · <TimeText value={e.at} />{e.reason && ` · ${e.reason}`}</li>)}</ul></details>}
  </section>;
}
