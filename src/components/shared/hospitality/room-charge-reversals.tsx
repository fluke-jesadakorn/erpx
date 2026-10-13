"use client";
import { useEffect, useState } from "react";
import { Button, Field, TextInput } from "@/components/ui/foundation";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { useFrontLanguage } from "@/components/front/front-language";
import type { PosWorkspace } from "@/server/modules/pos/public/pos";
import type { roomChargeReversalQueue } from "@/server/modules/pos/public/room-charge-reversal-queue";

type Send = (operation: string, input: Record<string, unknown>) => Promise<unknown>;
export function RoomChargeReversalRequest({ data, send, busy }: { data: PosWorkspace; send: Send; busy: boolean }) {
  const language = useFrontLanguage(), th = language === "th";
  if (!data.sale || !data.roomCharge || data.roomCharge.reversal_entry_id || !data.permissions["pos.sell"]) return null;
  return <FormDisclosure>
    <summary>{th ? "ขอย้อนยอดบัญชีผู้เข้าพัก" : "Request Guest Account charge reversal"}</summary>
    <p>{th ? "ผู้อนุมัติ POS และ PMS ต้องเป็นคนละคน เงินและสต็อกจะเปลี่ยนเมื่ออนุมัติครบแล้ว" : "Different POS and PMS approvers must review this request. Balances and stock change after both approvals."}</p>
    <form onSubmit={event => {
      event.preventDefault(); const form = new FormData(event.currentTarget);
      void send("room_charge_reversal.request", {id:data.sale!.id,expectedVersion:data.sale!.version,propertyId:data.roomCharge!.property_id,expectedPropertyVersion:data.roomCharge!.property_version,reason:String(form.get("reason")??""),restockLineIds:form.getAll("restockLine").map(String)});
    }}>
      <Field>{th ? "เหตุผล" : "Reason"}<TextInput name="reason" minLength={3} maxLength={1000} required /></Field>
      {data.lines.filter(line => line.kind === "stock" && line.fulfillment_state === "delivered").map(line => <Field key={String(line.id)} orientation="inline"><TextInput type="checkbox" name="restockLine" value={String(line.id)} />{th ? "คืนสต็อก" : "Restock"} {String(line.name)}</Field>)}
      <Button type="submit" disabled={busy}>{th ? "ส่งคำขอ" : "Submit request"}</Button>
    </form>
  </FormDisclosure>;
}
export function RoomChargeReversalReviews(props: { base: string; propertyId?: string }) {
  return <ReversalReviews key={`${props.base}:${props.propertyId}`} {...props}/>;
}
function ReversalReviews({ base, propertyId }: { base: string; propertyId?: string }) {
  const [rows,setRows] = useState<Awaited<ReturnType<typeof roomChargeReversalQueue>>>([]), [error,setError] = useState(""), [busy,setBusy] = useState(false);
  const language=useFrontLanguage(), th=language==="th";
  const api = `/api/v1${base}${base.startsWith("/front/") ? "" : "/pos"}`;
  useEffect(() => {
    const controller=new AbortController();
    void workFetch(`${api}/room-charge-reversals?${new URLSearchParams(propertyId?{property:propertyId}:{})}`,{signal:controller.signal}).then(async response => {
      const body=await response.json(); if(!response.ok) throw new Error(body.error?.message??"Review unavailable");
      if(!controller.signal.aborted)setRows(body.data);
    }).catch(error => {if(!controller.signal.aborted)setError(error.message);});
    return ()=>controller.abort();
  },[api,propertyId,busy]);
  async function decide(row: typeof rows[number], reject: boolean, reason: string) {
    setBusy(true); setError("");
    try {
      const response=await workFetch(`${api}/room_charge_reversal/${reject?"reject":"approve"}_${row.stage}`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":crypto.randomUUID()},body:JSON.stringify({id:row.id,expectedVersion:row.version,reason})});
      const body=await response.json(); if(!response.ok)throw new Error(body.error?.message??"Review failed");
    }catch(error){setError(error instanceof Error?error.message:String(error));}finally{setBusy(false);}
  }
  return <section><h3>{th?"คำขอย้อนยอดบัญชีผู้เข้าพักจาก POS":"POS Guest Account reversal requests"}</h3>
    {error && <p role="alert">{error}</p>}
    {!rows.length && !error && <p>{th?"ไม่มีคำขอรออนุมัติ":"No requests awaiting review."}</p>}
    {rows.map(row=><FormDisclosure key={`${row.id}:${row.version}`}><summary>{row.number} · {row.stage.toUpperCase()} · {row.reason}</summary>
      <p>{row.amount_minor/100} · {row.reasonDisabled??(th?"พร้อมตรวจอนุมัติ":"Ready for review")}</p>
      <form onSubmit={event=>{event.preventDefault();const form=new FormData(event.currentTarget);void decide(row,form.get("decision")==="reject",String(form.get("reason")??""));}}>
        <Field>{th?"เหตุผลการตรวจ":"Review reason"}<TextInput name="reason" minLength={3} required /></Field>
        <Button type="submit" name="decision" value="approve" disabled={busy||!row.canApprove}>{th?"อนุมัติ":"Approve"}</Button>
        <Button type="button" disabled={busy||!row.canAct} onClick={event=>{const form=event.currentTarget.closest("form");if(form?.reportValidity())void decide(row,true,String(new FormData(form).get("reason")??""));}}>{th?"ปฏิเสธ":"Reject"}</Button>
      </form>
    </FormDisclosure>)}
  </section>;
}
