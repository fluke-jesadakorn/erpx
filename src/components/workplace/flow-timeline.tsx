"use client";

import { workFetch } from "@/lib/work-fetch";
import { useState } from "react";
import { useRouter } from "next/navigation";
import type { FlowView } from "@/lib/flow";
import styles from "./flow.module.css";

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
  return <section className={`card ${styles.flow}`} aria-label="Workflow and next responsible staff">
    <header><div><p className="eyebrow">Flow · ทีมและผู้รับผิดชอบ</p><h2>{flow.current?.action ?? "No pending action"}</h2></div><span className="badge">สถานะ Flow: {flow.state.replaceAll("_"," ")}</span></header>
    {flow.current && <p>ตอนนี้: <strong>{flow.current.primary?.name ?? "ยังไม่ได้กำหนดคนหลัก"}</strong> · {flow.current.teamName} · {flow.current.role.replaceAll("_"," ")}{flow.canAct && <strong> · งานของคุณ</strong>}</p>}
    {flow.next && <p className="muted">ถัดไป: {flow.next.action} → {flow.next.teamName} / {flow.next.primary?.name ?? "รอกำหนดคนหลัก"}</p>}
    {flow.blocker && <p role="status" className="notice">{flow.blocker}</p>}
    {error && <p role="alert" className="error">{error}</p>}
    <ol className={styles.steps}>{flow.steps.map((step,index) => <li key={step.key} className={step.status === "current" ? styles.current : undefined} aria-current={step.status === "current" ? "step" : undefined}>
      <span className={styles.number}>{index+1}</span><div><strong>{step.action}</strong><p>{step.teamName} · {step.primary?.name ?? "Unassigned"}</p><small>{step.role.replaceAll("_"," ")} · {step.status === "current" ? "รอดำเนินการ" : step.status === "recorded" ? "มีประวัติการดำเนินการ" : "ขั้นตอนใน Flow"}</small>
      {flow.current && step.blocker && step.status !== "recorded" && <p className="muted">{step.blocker}</p>}
      {flow.canManage && flow.current && index >= flow.steps.indexOf(flow.current) && <details><summary>กำหนด / เปลี่ยนคนหลัก</summary><form onSubmit={event => void assign(event,step.key)}>
        <label>ผู้รับผิดชอบ<select name="userId" required defaultValue={step.primary?.userId ?? ""} disabled={busy}><option value="">เลือกผู้มีสิทธิ์</option>{step.candidates.map(p => <option key={p.userId} value={p.userId}>{p.name} · {p.teamName}</option>)}</select></label>
        <label>เหตุผล<input name="reason" required minLength={3} maxLength={2000} disabled={busy}/></label>
        <label className="check-row"><input name="saveDefault" type="checkbox" disabled={busy}/>ใช้เป็นค่าเริ่มต้นสำหรับเอกสารใหม่ในขอบเขตนี้ด้วย</label>
        <button className="button secondary" disabled={busy || !step.candidates.length}>บันทึกผู้รับผิดชอบ</button>
      </form></details>}
      </div></li>)}</ol>
    {!!flow.history.length && <details><summary>ประวัติส่งต่องาน ({flow.history.length})</summary><ul>{flow.history.map((e,index) => <li key={index}>{e.actorName} · {e.action} · {new Date(e.at).toLocaleString("en-GB",{timeZone:"Asia/Bangkok"})}{e.reason && ` · ${e.reason}`}</li>)}</ul></details>}
  </section>;
}
