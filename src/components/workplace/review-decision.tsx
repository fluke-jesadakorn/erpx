"use client";
import { useState } from "react";
import { Button,Field,TextArea } from "@/components/ui/foundation";
import type { ReviewDecision } from "@/lib/work_projection/work-review";
import { useFrontLanguage } from "@/components/front/front-language";
export function ReviewDecisionForm({actions,busy,note,onNote,onConfirm,minimum=()=>3}:{actions:ReviewDecision[];busy:boolean;note:string;onNote:(note:string)=>void;onConfirm:(decision:ReviewDecision)=>void;minimum?:(decision:ReviewDecision)=>number}){
 const [selected,setSelected]=useState<ReviewDecision|null>(null),th=useFrontLanguage()==="th";
 const labels={approve:th?"อนุมัติ":"Approve",reject:th?"ปฏิเสธ":"Reject",return:th?"ส่งกลับแก้ไข":"Return for correction"};
 return <section aria-label="Your decision"><h3>{th?"การตัดสินใจของคุณ":"Your decision"}</h3><div className="ui-form-actions">{actions.map(a=><Button key={a} type="button" appearance="standard" variant={a==="reject"?"danger":selected===a?"primary":"secondary"} aria-pressed={selected===a} disabled={busy} onClick={()=>setSelected(a)}>{labels[a]}</Button>)}</div>{selected&&<form onSubmit={e=>{e.preventDefault();if(note.trim().length>=minimum(selected))onConfirm(selected);}}><Field appearance="standard">{th?"เหตุผล":"Reason"}{minimum(selected)===0?th?" (ไม่บังคับ)":" (optional)":""}<TextArea appearance="standard" value={note} onChange={e=>onNote(e.target.value)} required={minimum(selected)>0} minLength={minimum(selected)||undefined} maxLength={2000} disabled={busy}/></Field><p className="muted">{th?"ยืนยันการตัดสินใจหลังตรวจข้อมูลและหลักฐานของรายการนี้":"Confirm after reviewing this record and its evidence."}</p><div className="ui-form-actions request-footer"><Button appearance="standard" clientManaged type="submit" variant={selected==="reject"?"danger":"primary"} disabled={busy||note.trim().length<minimum(selected)}>{busy?(th?"กำลังบันทึก…":"Saving…"):`${th?"ยืนยัน":"Confirm"} ${labels[selected]}`}</Button></div></form>}</section>;
}
