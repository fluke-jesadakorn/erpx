"use client";
import {useRef,useState} from "react";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {Button,Field,TextInput,type UiAppearance} from "@erpx/ui/components/foundation";
import {FormDisclosure} from "@erpx/ui/components/form-disclosure";
import {AttachmentPanel,type AttachmentPanelChange} from "@erpx/feature-files/components/attachment-panel";
import {workforceFormUnchanged} from "@erpx/feature-people_payroll/lib/workforce-command-client";
import type {WorkforcePurchase} from "@erpx/feature-people_payroll/lib/workforce-controls";
import {useWorkforceCommand} from "@erpx/feature-people_payroll/components/self-service/use-workforce-command";
import {WorkforceDraftBoundary} from "@erpx/feature-people_payroll/components/self-service/workforce-draft-boundary";
type Row=Record<string,unknown>;
export function TimeEntryEditor({appearance="standard",base,person,entries,canEdit,canApprove,userId,workContext,purchase}:{appearance?:UiAppearance;base:string;person:Row;entries:Row[];canEdit:boolean;canApprove:boolean;userId:string;workContext:string;purchase:WorkforcePurchase|undefined}){
 const router=useRouter(),command=useWorkforceCommand(base,`${workContext}:time:${String(person.engagement_id)}`,purchase),{busy,error,notice,pending}=command;
 const [editing,setEditing]=useState<Row|null>(null),[open,setOpen]=useState(false),[changed,setChanged]=useState(false),[revision,setRevision]=useState(0),[correctionFiles,setCorrectionFiles]=useState<AttachmentPanelChange|null>(null);
 const revisionRef=useRef(0);
 function discard(){setEditing(null);setCorrectionFiles(null);setChanged(false);setRevision(value=>value+1);revisionRef.current++;}
 async function finish(result:NonNullable<Awaited<ReturnType<typeof command.send>>>,submittedRevision:number){
  if(result.completion&&workforceFormUnchanged(result.completion)&&submittedRevision===revisionRef.current){if(["save","create"].includes(result.request.action)){discard();setOpen(false);}else result.completion.form.reset();}
  else command.setNotice("The original time action was saved. Any newer local draft is retained; review the current time entry before submitting it.");
  router.refresh();
 }
 return <WorkforceDraftBoundary dirty={changed||Boolean(pending)||Boolean(correctionFiles?.readyCount)}><section data-ui-appearance={appearance} className={appearance==="standard"?"ui-panel":"card"}>
  <h2>Time entries</h2>{error&&<p role="alert">{error}</p>}{notice&&<p role="status">{notice}</p>}
  {pending&&<div role="status"><p>Check the original Workforce action before submitting another time entry.</p><Button appearance={appearance} disabled={busy} onClick={async()=>{const current=revisionRef.current,result=await command.recover();if(result)await finish(result,current);}}>Check original action</Button></div>}
  {canEdit&&<FormDisclosure appearance="standard" open={open} onToggle={event=>setOpen(event.currentTarget.open)} drawerTitle={editing?"Correct time entry":"Submit time entry"} dismissDisabled={busy} onDiscard={discard} error={error}>
   <summary>{editing?"Continue time correction":"Submit time entry"}</summary>
   <form key={`${String(editing?.id??"new")}:${revision}`} className="ui-form-grid" onChange={()=>{setChanged(true);revisionRef.current++;}} onSubmit={async event=>{
    event.preventDefault();const form=new FormData(event.currentTarget),submittedRevision=revisionRef.current;
    const result=await command.send({kind:"time",action:editing?"save":"create",...(editing?{id:String(editing.id)}:{}),input:{...(editing?{expectedVersion:editing.version,attachmentSetIds:correctionFiles?.setId&&correctionFiles.readyCount>0&&correctionFiles.bindable?[correctionFiles.setId]:[]}:{engagementId:person.engagement_id}),workDate:form.get("date"),regularHours:Number(form.get("regular")),overtimeHours:Number(form.get("overtime")),note:form.get("note")}});
    if(result)await finish(result,submittedRevision);
   }}>
    <Field appearance={appearance}>Work date<TextInput appearance={appearance} name="date" type="date" required disabled={busy||!command.allowed("save")} defaultValue={String(editing?.work_date??"")}/></Field>
    <Field appearance={appearance}>Regular hours<TextInput appearance={appearance} name="regular" type="number" min="0" max="24" step="0.01" required disabled={busy||!command.allowed("save")} defaultValue={String(editing?.regular_hours??8)}/></Field>
    <Field appearance={appearance}>Overtime hours<TextInput appearance={appearance} name="overtime" type="number" min="0" max="24" step="0.01" required disabled={busy||!command.allowed("save")} defaultValue={String(editing?.overtime_hours??0)}/></Field>
    <Field appearance={appearance}>Work note<TextInput appearance={appearance} name="note" maxLength={500} disabled={busy||!command.allowed("save")} defaultValue={String(editing?.note??"")}/></Field>
    {editing?.status==="rejected"&&<AttachmentPanel appearance={appearance} base={base} targetType="time_entry" targetId={String(editing.id)} expectedTargetVersion={Number(editing.version)} componentKind="action" componentRef="correction-resubmission" purposeKey="corrected_entry_proof" title="Correction proof" description="Attach the source document or image supporting this corrected time entry." visibility="Visible only to the employee and authorized time reviewers." disabled={busy} readOnly={!command.allowed("save")} onChange={setCorrectionFiles}/>}
    <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy||!command.allowed("save")||Boolean(correctionFiles?.links.length&&!correctionFiles.bindable)}>{editing?"Save and resubmit time":"Submit time for approval"}</Button>
   </form>
  </FormDisclosure>}
  {entries.map(entry=><article key={String(entry.id)}><div className="ui-record-item"><strong>{String(entry.work_date)}</strong><span>{String(entry.regular_hours)}h + {String(entry.overtime_hours)}h overtime · {String(entry.status)}</span>
   {canEdit&&entry.submitted_by===userId&&["draft","submitted","rejected"].includes(String(entry.status))&&<Button appearance={appearance} disabled={busy||changed||!command.allowed("save")} onClick={()=>{setEditing(entry);setCorrectionFiles(null);setOpen(true);}}>Edit time</Button>}
   {canApprove&&entry.status==="submitted"&&entry.submitted_by!==userId&&person.user_id!==userId&&<>
    <Button appearance={appearance} disabled={busy||!command.allowed("approve")} onClick={async()=>{const result=await command.send({kind:"time",action:"approve",id:String(entry.id),input:{expectedVersion:entry.version}});if(result)router.refresh();}}>Approve time</Button>
    <FormDisclosure appearance="standard" drawerTitle="Return time for correction" dismissDisabled={busy} error={error}><summary>Return for correction</summary><form className="ui-form-grid" onSubmit={async event=>{event.preventDefault();const current=revisionRef.current,result=await command.send({kind:"time",action:"reject",id:String(entry.id),input:{expectedVersion:entry.version,reason:new FormData(event.currentTarget).get("reason")}});if(result)await finish(result,current);}}><Field appearance={appearance}>Correction reason<TextInput appearance={appearance} name="reason" required minLength={3} maxLength={500} disabled={busy||!command.allowed("reject")}/></Field><Button appearance={appearance} clientManaged type="submit" disabled={busy||!command.allowed("reject")}>Return time entry</Button></form></FormDisclosure>
   </>}
  </div>{Boolean(entry.note)&&<p className="muted">{String(entry.note)}</p>}{Boolean(entry.rejection_reason)&&<p>Correction requested: {String(entry.rejection_reason)}</p>}</article>)}
  <p className="muted">Approved time is locked and feeds payroll calculations. Corrections require a new review.</p>
 </section></WorkforceDraftBoundary>;
}
