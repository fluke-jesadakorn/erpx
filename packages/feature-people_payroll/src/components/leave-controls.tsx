"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import {FormDisclosure} from "@erpx/ui/components/form-disclosure";
import {useWorkforceCommand} from "@erpx/feature-people_payroll/components/self-service/use-workforce-command";
import {WorkforceDraftBoundary} from "@erpx/feature-people_payroll/components/self-service/workforce-draft-boundary";
import {workforceFormUnchanged} from "@erpx/feature-people_payroll/lib/workforce-command-client";
import type {WorkforcePurchase} from "@erpx/feature-people_payroll/lib/workforce-controls";
import { Button, Field, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { Card, FormLayout, Stack } from "@erpx/ui/components/foundation";
export function LeaveControls({
  base,
  workContext,
  purchase,
  recordId,
  version,
  canReject = false,
  canCancel = false,
  initialOpen = false,
}: {
  base: string;
  workContext:string;
  purchase:WorkforcePurchase|undefined;
  recordId?: string;
  version?: number;
  canReject?: boolean;
  canCancel?: boolean;
  initialOpen?: boolean;
}) {
  const router=useRouter(),command=useWorkforceCommand(base,`${workContext}:leave:${recordId??"new"}:legacy`,purchase),{busy,error,notice,pending}=command;
  const [open,setOpen]=useState(initialOpen);
  async function completed(result:NonNullable<Awaited<ReturnType<typeof command.send>>>){
    if(result.completion&&workforceFormUnchanged(result.completion)){setOpen(false);if(!recordId)router.push(canonicalUiHref(`${base}/tasks/people_payroll/${result.receipt.id}`));else router.refresh();}
    else command.setNotice("The original leave action was saved. Your newer draft is retained.");
  }
  async function submit(form:FormData,action?:string){const result=await command.send({kind:"leave",action:recordId?action??"cancel":"create",...(recordId?{id:recordId}:{}),input:recordId?{action,expectedVersion:version,note:String(form.get("note")??"")}:{startDate:String(form.get("startDate")),days:Number(form.get("days")),handoverNote:String(form.get("handoverNote"))}});if(result)await completed(result);}
  const status=<>{notice&&<InlineNotice role="status">{notice}</InlineNotice>}{pending&&<InlineNotice role="status"><p>Check the original leave action before submitting another draft.</p><Button appearance="standard" disabled={busy} onClick={async()=>{const result=await command.recover();if(result)await completed(result);}}>Check original action</Button></InlineNotice>}</>;
  if(recordId)return <WorkforceDraftBoundary dirty={Boolean(pending)}><Card as="section" appearance="standard" className="ui-panel">{status}<FormLayout className="ui-form-grid" onSubmit={event=>event.preventDefault()}>
    <Field appearance="standard">Decision note<TextArea appearance="standard" name="note" maxLength={2048}/></Field>
    <div className="ui-form-actions">{canReject&&<Button appearance="standard" disabled={busy||!command.allowed("reject")} onClick={event=>void submit(new FormData(event.currentTarget.form!),"reject")}>Reject request</Button>}{canCancel&&<Button appearance="standard" disabled={busy||!command.allowed("cancel")} onClick={event=>void submit(new FormData(event.currentTarget.form!),"cancel")}>Cancel request</Button>}</div>
    {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
  </FormLayout></Card></WorkforceDraftBoundary>;
  return <WorkforceDraftBoundary dirty={Boolean(pending)}><Card as="section" appearance="standard" className="ui-panel">{status}
    <FormDisclosure appearance="standard" inline={initialOpen} open={open} onToggle={event=>setOpen(event.currentTarget.open)} drawerTitle="Request leave" dismissDisabled={busy} error={error}><summary>Request leave</summary><Stack as="div" className="ui-stack"><FormLayout className="ui-form-grid" onSubmit={event=>{event.preventDefault();void submit(new FormData(event.currentTarget));}}>
      <Field appearance="standard">First day<TextInput appearance="standard" type="date" name="startDate" required disabled={busy||!command.allowed("create")}/></Field>
      <Field appearance="standard">Requested working days<TextInput appearance="standard" type="number" name="days" min={1} max={365} defaultValue={1} required disabled={busy||!command.allowed("create")}/></Field>
      <Field appearance="standard">Handover plan<TextArea appearance="standard" name="handoverNote" maxLength={2048} disabled={busy||!command.allowed("create")}/></Field>
      <p className="muted">Enter working days under your Company’s leave policy. Your department approver reviews first, then an independent HR administrator.</p>
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||!command.allowed("create")}>Save leave draft</Button>
      {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    </FormLayout></Stack></FormDisclosure>
  </Card></WorkforceDraftBoundary>;
}
