"use client";
import { useEffect, useLayoutEffect, useState } from "react";
import { Button, Field, TextInput } from "@/components/ui/foundation";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { useFrontLanguage } from "@/components/front/front-language";
import type { PosWorkspace } from "@/server/modules/pos/public/pos";
import { createRoomChargeReviewCommands, roomChargeDecisionSource, type RoomChargeDecisionSource, type RoomChargeReviewOwner, type RoomChargeReviewPending, type RoomChargeReviewRow, type RoomChargeReviewSource } from "@/lib/pos/room-charge-review-command";
import { registerUnsavedWork } from "@/lib/work_projection/context-switch";

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
type ReviewsProps = { base: string; propertyId?: string; owner: RoomChargeReviewOwner };
export function RoomChargeReversalReviews(props: ReviewsProps) {
  return <ReversalReviews key={JSON.stringify([props.base,props.propertyId,props.owner.userId,props.owner.companyId])} {...props}/>;
}
function ReversalReviews({ base, propertyId, owner }: ReviewsProps) {
  const [rows,setRows] = useState<RoomChargeReviewRow[]>([]), [error,setError] = useState(""), [notice,setNotice] = useState(""), [busy,setBusy] = useState(false), [readyFor,setReadyFor] = useState<string|null>(null);
  const [selected,setSelected] = useState<RoomChargeDecisionSource|null>(null), [reason,setReason] = useState(""), [open,setOpen] = useState(false), [dirty,setDirty] = useState(false), [pending,setPending] = useState<RoomChargeReviewPending|null>(null);
  // The command reader and its committed source have one mounted lifetime.
  // Construction is pure: no DOM/storage/owner read runs in the initializer.
  const [runtime] = useState(()=>{
    let committed:RoomChargeReviewSource|null=null, generation=0, readSequence=0;
    const identity=(value:RoomChargeReviewSource)=>JSON.stringify([value.base,value.propertyId,value.owner.userId,value.owner.companyId]);
    function ownerLease(){
      const original=committed?identity(committed):null, epoch=generation;
      let cancelled=false;
      return {current:()=>!cancelled&&!!committed&&original===identity(committed)&&epoch===generation,cancel(){cancelled=true;}};
    }
    const commands=createRoomChargeReviewCommands(()=>committed);
    return {commands,
      commit(value:RoomChargeReviewSource){if(!committed||identity(committed)!==identity(value)){++generation;++readSequence;}committed=value;},
      dispose(){committed=null;++generation;++readSequence;},
      isActive:()=>committed!==null,
      ownerLease,
      cancelReads(){++readSequence;},
      beginRead(){
        const token=++readSequence, lease=ownerLease();
        return {current:()=>token===readSequence&&lease.current(),cancel(){lease.cancel();if(token===readSequence)++readSequence;}};
      },
    };
  });
  const {commands}=runtime;
  const language=useFrontLanguage(), th=language==="th", ownerStamp=JSON.stringify([base,propertyId,owner]), ready=readyFor===ownerStamp;
  useLayoutEffect(()=>{runtime.commit({base,propertyId,owner,rows});});
  useLayoutEffect(()=>()=>runtime.dispose(),[runtime]);
  function retain(){try{const value=commands.pending();if(runtime.isActive())setPending(value);return value;}catch(cause){if(runtime.isActive())setError(cause instanceof Error?cause.message:String(cause));return null;}}
  useEffect(()=>{
    const lease=runtime.ownerLease();
    // Storage is an external source. Restore only while this committed owner
    // still owns the mounted controller; StrictMode cleanup cancels its task.
    queueMicrotask(()=>{
      if(!lease.current())return;
      try{const original=commands.pending();if(original&&lease.current()){setPending(original);setSelected(original.source);setReason((JSON.parse(original.body) as {reason:string}).reason);setNotice("Your original decision is retained. Use Retry original decision to recover its result.");}}catch(cause){if(lease.current())setError(cause instanceof Error?cause.message:String(cause));}
    });
    const unregister=registerUnsavedWork(()=>{try{return commands.busy()||!!commands.pending();}catch{return true;}});
    return()=>{lease.cancel();unregister();};
  },[commands,runtime]);
  useEffect(()=>{
    const lease=runtime.beginRead();
    void commands.read().then(result=>{result.assertCurrent();if(lease.current()){setRows(result.rows);setReadyFor(ownerStamp);setError("");}}).catch(cause=>{if(lease.current())setError(cause instanceof Error?cause.message:String(cause));});
    return()=>lease.cancel();
  },[commands,runtime,ownerStamp]);
  async function reload(){if(busy)return;const lease=runtime.beginRead();setReadyFor(null);try{const result=await commands.read();result.assertCurrent();if(lease.current()){setRows(result.rows);setReadyFor(ownerStamp);setError("");}}catch(cause){if(lease.current())setError(cause instanceof Error?cause.message:String(cause));}}
  function choose(row:RoomChargeReviewRow){
    if(busy||pending){setError(th?"ตรวจคำตอบของคำขอเดิมก่อนเลือกคำขออื่น":"Recover the original decision before selecting another request.");return;}
    if(dirty&&!window.confirm(th?"ทิ้งเหตุผลที่ยังไม่ได้ส่งหรือไม่":"Discard the newer unsubmitted review reason?"))return;
    setSelected(roomChargeDecisionSource(row));setReason("");setDirty(false);setOpen(true);setError("");setNotice("");
  }
  async function decide(reject:boolean,form?:HTMLFormElement,retry=false){
    if(busy||!selected)return;runtime.cancelReads();setBusy(true);setError("");setNotice("");
    try{
      const result=retry?await commands.retry():await commands.execute(selected,reject,reason,form);
      result.assertCurrent();const acknowledged=result.acknowledge();
      if(!acknowledged)return;
      if(runtime.isActive()){setRows(result.rows);setReadyFor(ownerStamp);setPending(null);if(acknowledged.newer){setNotice(th?"บันทึกคำขอเดิมแล้ว เก็บเหตุผลที่แก้ไขใหม่ไว้โดยยังไม่ส่ง":"The original decision was saved. Your newer review draft is retained and has not been submitted.");}else{setDirty(false);setReason("");setOpen(false);setSelected(null);setNotice(th?"บันทึกการตรวจแล้ว":"Review decision saved.");}}
    }catch(cause){if(runtime.isActive())setError(cause instanceof Error?cause.message:String(cause));}
    finally{if(runtime.isActive()){setBusy(false);retain();}}
  }
  const current=selected?rows.find(row=>row.id===selected.id&&row.version===selected.version&&row.stage===selected.stage):undefined;
  const retry=<Button appearance="standard" type="button" disabled={busy} onClick={()=>void decide(false,undefined,true)}>{th?"ลองคำขอเดิม":"Retry original decision"}</Button>;
  return <section data-room-charge-review-pending={pending?"true":undefined}>
    <h3>{th?"คำขอย้อนยอดบัญชีผู้เข้าพักจาก POS":"POS Guest Account reversal requests"}</h3>
    {error&&<p role="alert">{error}</p>}{notice&&<p role="status">{notice}</p>}
    {pending&&<div><p>{th?"คำขอเดิมยังไม่ยืนยัน ห้ามส่งเหตุผลใหม่แทนคำขอนี้":"The original decision is not yet acknowledged. Keep its key and body while recovering."}</p>{retry}<Button appearance="standard" type="button" disabled={busy} onClick={()=>setOpen(true)}>{th?"เปิดรายละเอียดเดิม":"Open retained decision"}</Button></div>}
    <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>void reload()}>{th?"อ่านคิวปัจจุบัน":"Refresh current queue"}</Button>
    {!rows.length&&ready&&!error&&<p>{th?"ไม่มีคำขอรออนุมัติ":"No requests awaiting review."}</p>}
    {rows.map(row=><article key={row.id}><p>{row.number} · {row.stage.toUpperCase()} · {row.reason}</p><p>{row.amount_minor/100} · {row.reasonDisabled??(th?"พร้อมตรวจอนุมัติ":"Ready for review")}</p><Button appearance="standard" type="button" disabled={busy||!!pending||!ready||!row.canAct} onClick={()=>choose(row)}>{th?"ตรวจคำขอ":"Review request"}</Button></article>)}
    {selected&&<FormDisclosure appearance="standard" open={open} onToggle={event=>setOpen(event.currentTarget.open)} retainFormContents={!!pending||dirty} dismissDisabled={busy} error={error} discardLabel={pending?(th?"ปิดและเก็บคำขอเดิม":"Close and retain decision"):undefined} onDiscard={pending?undefined:()=>{setReason("");setDirty(false);}}>
      <summary>{selected.number} · {selected.stage.toUpperCase()} · {th?"ตรวจคำขอย้อนยอด":"Review charge reversal"}</summary>
      <p>{th?"คำขอและรายการขายเดิม":"Original request and linked sale"}: {selected.id} · {selected.saleId}</p>
      {notice&&<p role="status">{notice}</p>}
      {pending&&<div><p>{th?"การลองคำขอเดิมจะใช้เหตุผลและเวอร์ชันเดิม ไม่ใช้ข้อมูลที่แก้ไขใหม่":"Retry uses the original reason and versions. It does not submit your newer fields."}</p>{retry}</div>}
      <form onSubmit={event=>{event.preventDefault();void decide(false,event.currentTarget);}}>
        <Field>{th?"เหตุผลการตรวจ":"Review reason"}<TextInput name="reason" value={reason} minLength={3} maxLength={1000} required onChange={event=>{setReason(event.target.value);setDirty(true);}}/></Field>
        <Button appearance="standard" type="submit" disabled={busy||!!pending||!ready||!current?.canApprove}>{th?"อนุมัติ":"Approve"}</Button>
        <Button appearance="standard" type="button" disabled={busy||!!pending||!ready||!current?.canAct} onClick={event=>{const form=event.currentTarget.closest("form");if(form?.reportValidity())void decide(true,form);}}>{th?"ปฏิเสธ":"Reject"}</Button>
      </form>
      <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>setOpen(false)}>{th?"ปิดและเก็บร่าง":"Close and retain draft"}</Button>
    </FormDisclosure>}
  </section>;
}
