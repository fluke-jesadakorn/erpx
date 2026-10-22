"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { createContext, useContext, useEffect, useLayoutEffect, useId, useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import WorkspaceLink from "@erpx/workplace/components/workspace-link";
import {createFieldServiceCommands,fieldServiceOwner,type FieldServiceSource} from "@erpx/feature-field_service/lib/command";
import {registerUnsavedWork} from "@erpx/feature-work_projection/lib/context-switch";
import { PreferredDateTimeField } from "@erpx/feature-people_payroll/components/time/preferred-date-time-field";
import { decimalToMinor, minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { ServiceSpareForm } from "@erpx/feature-field_service/components/service-spare-form";
import { ServiceAgreementReferenceFields, ServiceRequestReferenceFields, ServiceOrderReferenceFields, ServiceTechnicianField } from "@erpx/feature-field_service/components/service-reference-fields";
import type { fieldServiceDetail, fieldServiceWorkspace } from "@erpx/server-field_service/public/field-service";
import { FormLayout, Card } from "@erpx/ui/components/foundation";
function useCommand(current:()=>FieldServiceSource|null,rendered:FieldServiceSource) {
  const router=useRouter(),commands=useRef<ReturnType<typeof createFieldServiceCommands>|null>(null),inFlight=useRef(false);commands.current??=createFieldServiceCommands(current);
  const [messageOwner,setMessageOwner]=useState<string|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(""),[uncertain,setUncertain]=useState(false),[notice,setNotice]=useState(""),[saved,setSaved]=useState<{owner:string;id:string;href:string;agreement:boolean}|null>(null);
  useEffect(()=>registerUnsavedWork(()=>busy||uncertain),[busy,uncertain]);
  async function send(input:unknown,form?:HTMLFormElement,retry=false,displayed?:FieldServiceSource) {
    if(inFlight.current)return;const source=current();if(!source)return;const capturedOwner=fieldServiceOwner(source);inFlight.current=true;setMessageOwner(capturedOwner);setBusy(true);setError("");setNotice("");
    try {
      const outcome=await(retry?commands.current!.retry():commands.current!.submit(input,form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined,displayed)),unchanged=outcome.unchanged();
      if(!outcome.acknowledge())throw Error("Confirm the original Service request again.");setUncertain(false);
      const href=canonicalUiHref(`${source.base}/field-service${outcome.agreement?'':`/${encodeURIComponent(outcome.receipt.id)}`}`);setSaved({owner:capturedOwner,id:outcome.receipt.id,href,agreement:outcome.agreement});setNotice(unchanged?"The original Service request was saved.":"The original Service request was saved. Your newer entries remain unsent.");
      if(unchanged){outcome.completion()?.form.reset();try{router.push(href);router.refresh();}catch{setError("Saved, but navigation failed. Open the saved original record without submitting a replacement request.");}}
    } catch(cause) {setUncertain(commands.current!.pending());setError(cause instanceof Error?cause.message:"The result is uncertain. Confirm the original Service request.");}
    finally {inFlight.current=false;setBusy(false);}
  }
  const visible=saved&&saved.owner===fieldServiceOwner(rendered)?saved:null;
  const sameOwner=messageOwner===fieldServiceOwner(rendered);
  return {send,busy,error:sameOwner?error:"",uncertain,notice:sameOwner?notice:"",saved:visible,retry:()=>send(undefined,undefined,true),clearError:()=>setError(""),fail:(cause:unknown)=>{setMessageOwner(fieldServiceOwner(rendered));setError(cause instanceof Error?cause.message:"Review the form values.");}};
}
function ServiceConsole({
  base,
  data,
  detail,
  recovery,
}: {
  recovery:ReturnType<typeof useCommand>;
  base: string;
  data?: Awaited<ReturnType<typeof fieldServiceWorkspace>>;
  detail?: Awaited<ReturnType<typeof fieldServiceDetail>>;
}) {
  const source = data ?? detail!,
    { busy, error, clearError, fail } = recovery,
    order = detail?.order;
  const send=(input:unknown,form?:HTMLFormElement)=>recovery.send(input,form,false,serviceSource({base,data,detail}));
  function action(name: string, title: string, extra?: React.ReactNode) {
    if (!order || !(name === "release-billing" ? Number(order.can_release_billing) : Number(order.can_work))) return null;
    return (
      <ServiceDraftDrawer title={title} busy={busy} error={error}><FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const f = Object.fromEntries(new FormData(e.currentTarget));
          void send({
            ...f,
            id: order!.id,
            action: name,
            expectedVersion: Number(order!.version),
            scheduledAt: f.scheduledAt ? new Date(String(f.scheduledAt)).getTime() : undefined,
            expiresAt: f.expiresAt ? new Date(String(f.expiresAt)).getTime() : undefined,
            quantityMilli: f.quantityMilli ? Number(f.quantityMilli) : undefined,
            laborMinutes: f.laborMinutes ? Number(f.laborMinutes) : undefined,
          },e.currentTarget);
        }}
      >
        <h2>{title}</h2>
        {extra}
        <Field appearance="standard">
          Reason and evidence
          <TextArea appearance="standard" name="reason" required minLength={3} />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
          {title}
        </Button>
      </FormLayout></ServiceDraftDrawer>
    );
  }
  return (
    <ServiceRecoveryContext.Provider value={recovery}><ServiceCommandErrorContext.Provider value={clearError}>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {detail?.billing && (
        <Card as="section" appearance="standard" className="ui-panel" aria-label="Billing handoff">
          <h2>Billing handoff</h2>
          <p>
            Finance obligation: {String(detail.billing.currency)}{" "}
            {minorToDecimal(Number(detail.billing.amount_minor))} · {String(detail.billing.status)}
          </p>
          {Boolean(
            Number(detail.billing.warranty_applied) === 1 ||
            String(detail.billing.warranty_applied) === "true",
          ) ? (
            <p className="muted">
              Warranty pricing applied: {Number(detail.billing.warranty_discount_bps) / 100}%
              discount; coverage through {String(detail.billing.warranty_end_snapshot)} (calculated{" "}
              {String(detail.billing.pricing_date)}).
            </p>
          ) : (
            <p className="muted">
              Chargeable visit rate used; no active warranty discount was applied.
            </p>
          )}
        </Card>
      )}
      {data && (
        <>
          {source.canCreateAgreement && <ServiceDraftDrawer title="Draft service agreement" busy={busy} error={error}><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget));
              try { void send({
                ...f,
                entity: "agreement",
                departmentId: f.departmentId || null,
                assetId: f.assetId || null,
                visitRateMinor: decimalToMinor(String(f.visitRate)),
                warrantyDiscountBps: decimalToMinor(String(f.warrantyDiscountPercent)),
              },e.currentTarget); } catch (cause) { fail(cause); }
            }}
          >
            <h2>Draft service agreement</h2>
            <ServiceAgreementReferenceFields source={source} />
            <Field appearance="standard">
              Title
              <TextInput appearance="standard" name="title" required />
            </Field>
            <Field appearance="standard">
              Start
              <TextInput appearance="standard" name="startDate" type="date" required />
            </Field>
            <Field appearance="standard">
              End
              <TextInput appearance="standard" name="endDate" type="date" required />
            </Field>
            <Field appearance="standard">
              Visit rate
              <TextInput appearance="standard" name="visitRate" type="number" min="0" step="0.01" required />
            </Field>
            <Field appearance="standard">
              Warranty discount (%)
              <TextInput appearance="standard"
                name="warrantyDiscountPercent"
                type="number"
                min="0"
                max="100"
                step="0.01"
                defaultValue="0"
                required
              />
              <small className="muted">
                0 = full rate; 100 = free while the linked asset is under warranty.
              </small>
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Draft agreement
            </Button>
          </FormLayout></ServiceDraftDrawer>}
          {source.canCreateOrder && <ServiceDraftDrawer title="Request field service" busy={busy} error={error}><FormLayout className="ui-panel ui-form-grid"
            onSubmit={event=>{event.preventDefault();const fields=Object.fromEntries(new FormData(event.currentTarget)),agreement=source.agreements.find(row=>row.id===fields.agreementId);
              void send({...fields,entity:"request",agreementVersion:Number(agreement?.version),scheduledAt:fields.scheduledAt?new Date(String(fields.scheduledAt)).getTime():null},event.currentTarget);}}>
            <h2>Request field service</h2>
            <ServiceRequestReferenceFields source={source} />
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Request visit</Button>
          </FormLayout></ServiceDraftDrawer>}
          {source.canCreateOrder && <ServiceDraftDrawer title="Create service order" busy={busy} error={error}><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget)),
                ticket = source.tickets.find((x) => x.id === f.ticketId),
                agreement = source.agreements.find((x) => x.id === f.agreementId);
              void send({
                ...f,
                ticketVersion: Number(ticket?.version),
                agreementVersion: Number(agreement?.version),
                assetId: agreement?.asset_id || f.assetId || null,
                scheduledAt: f.scheduledAt ? new Date(String(f.scheduledAt)).getTime() : null,
              },e.currentTarget);
            }}
          >
            <h2>Create service order</h2>
            <ServiceOrderReferenceFields source={source} />
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create service order
            </Button>
          </FormLayout></ServiceDraftDrawer>}
        </>
      )}
      {data?.agreements
        .filter((x) => x.status === "draft" ? Number(x.can_activate) || Number(x.can_reject) : x.status === "active" && Number(x.can_retire))
        .map((x) => (
          <ServiceDraftDrawer key={String(x.id)} title={`Agreement review · ${String(x.title)}`} busy={busy} error={error}><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget));
              void send({
                ...f,
                entity: "agreement",
                id: x.id,
                expectedVersion: Number(x.version),
              },e.currentTarget);
            }}
          >
            <h2>Agreement review · {String(x.title)}</h2>
            <Field appearance="standard">
              Decision
              <Select appearance="standard" name="action">
                {x.status === "draft" && Number(x.can_activate)>0 && <option value="activate">Activate</option>}
                {x.status === "draft" && Number(x.can_reject)>0 && <option value="reject">Reject</option>}
                {x.status === "active" && Number(x.can_retire)>0 && <option value="retire">Retire</option>}
              </Select>
            </Field>
            <Field appearance="standard">
              Reason
              <TextArea appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !Number(x.can_review)}>
              Record decision
            </Button>
          </FormLayout></ServiceDraftDrawer>
        ))}
      {order &&
        ["open", "scheduled"].includes(String(order.status)) &&
        action(
          "schedule",
          "Schedule or reassign",
          <>
            <ServiceTechnicianField source={source} branchId={String(order.branch_id)} departmentId={order.department_id?String(order.department_id):null} defaultUserId={String(order.technician_id)} />
            <PreferredDateTimeField appearance="standard" name="scheduledAt" label="Schedule" required />
          </>,
        )}
      {order && detail && source.canUseParts && Number(order.can_work)>0 && ["open","scheduled"].includes(String(order.status)) &&
        <ServiceDraftDrawer title="Reserve spare parts" busy={busy} error={error}><ServiceSpareForm mode="reserve" spares={detail.spares} busy={busy}
          onSubmit={(input,form) => { void send({ ...input,id:order.id,action:"reserve-part",expectedVersion:Number(order.version) },form); }} /></ServiceDraftDrawer>}
      {order && order.status === "scheduled" && action("dispatch", "Dispatch technician")}
      {order && order.status === "dispatched" && action("start", "Start visit")}
      {order &&
        ["dispatched", "in_visit"].includes(String(order.status)) &&
        action("exception", "Record exception")}
      {order && order.status === "exception" && action("resume", "Resume visit")}
      {order && detail && source.canUseParts && Number(order.can_work)>0 && order.status === "in_visit" && <ServiceDraftDrawer title="Issue spare parts" busy={busy} error={error}><ServiceSpareForm spares={detail.spares}
        reservations={detail.parts.filter(x => Number(x.quantity_milli)>Number(x.consumed_milli)+Number(x.released_milli)).map(x => ({ id:String(x.id),warehouseId:String(x.warehouse_id),productId:String(x.product_id),remainingMilli:Number(x.quantity_milli)-Number(x.consumed_milli)-Number(x.released_milli) }))}
        busy={busy} onSubmit={(input,form) => { void send({ ...input,id:order.id,action:"issue-part",expectedVersion:Number(order.version) },form); }} /></ServiceDraftDrawer>}
      {order &&
        order.status === "in_visit" &&
        action(
          "complete",
          "Complete visit",
          <>
            <Field appearance="standard">
              Labor minutes
              <TextInput appearance="standard" name="laborMinutes" type="number" min="0" required />
            </Field>
            <Field appearance="standard">
              Completion evidence
              <TextArea appearance="standard" name="evidence" required minLength={3} />
            </Field>
            <Field appearance="standard">
              Customer confirmation
              <TextArea appearance="standard" name="customerConfirmation" required minLength={3} />
            </Field>
          </>,
        )}
      {order && order.status === "completed" && action("release-billing", "Release billing")}
      {order &&
        !["completed", "billed", "cancelled"].includes(String(order.status)) &&
        action("cancel", "Cancel service order")}
    </ServiceCommandErrorContext.Provider></ServiceRecoveryContext.Provider>
  );
}

type ServiceConsoleProps = { base: string; data?: Awaited<ReturnType<typeof fieldServiceWorkspace>>; detail?: Awaited<ReturnType<typeof fieldServiceDetail>> };
const ServiceEditorContext = createContext<(id: string, open: boolean) => void>(() => {});
const ServiceCommandErrorContext = createContext<() => void>(() => {});
const ServiceRecoveryContext=createContext<ReturnType<typeof useCommand>|null>(null);
function serviceSource(props:ServiceConsoleProps):FieldServiceSource {const source=props.data??props.detail!,targets:FieldServiceSource["targets"]={};for(const row of source.agreements)targets[`agreement:${row.id}`]={version:Number(row.version),status:String(row.status)};for(const row of source.orders)targets[`order:${row.id}`]={version:Number(row.version),status:String(row.status)};for(const row of source.tickets)targets[`ticket:${row.id}`]={version:Number(row.version),status:String(row.status)};if(props.detail)targets[`order:${props.detail.order.id}`]={version:Number(props.detail.order.version),status:String(props.detail.order.status)};return {base:props.base,workspaceKey:source.workspaceKey,recordId:props.detail?String(props.detail.order.id):null,targets};}
export function FieldServiceConsole(props: ServiceConsoleProps) {
  const source = props.data ?? props.detail!;
  const live=useRef<FieldServiceSource|null>(serviceSource(props));useLayoutEffect(()=>{live.current=serviceSource(props);return()=>{live.current=null;};},[props]);
  const recovery=useCommand(()=>live.current,serviceSource(props));
  return <ServiceEditorSession key={`${source.workspaceKey}:${props.detail?.order.id ?? "workspace"}`} {...props} recovery={recovery} />;
}
function ServiceEditorSession(props: ServiceConsoleProps&{recovery:ReturnType<typeof useCommand>}) {
  const [snapshot, setSnapshot] = useState<(ServiceConsoleProps&{recovery:ReturnType<typeof useCommand>}) | null>(null);
  const editors = useRef(new Set<string>());
  function editorChanged(id: string, open: boolean) {
    if (open) {
      if (!editors.current.size) setSnapshot(props);
      editors.current.add(id);
    } else {
      editors.current.delete(id);
      if (!editors.current.size) setSnapshot(null);
    }
  }
  // A refreshed status or permission must not remove the parent of an open draft.
  // Its captured versions still meet the current server guards on submission.
  // Interaction state continues rendering; do not freeze the form's React nodes.
  return <ServiceEditorContext.Provider value={editorChanged}><ServiceConsole {...(snapshot ?? props)} recovery={props.recovery} /></ServiceEditorContext.Provider>;
}
function ServiceDraftDrawer({ title, busy, error, children }: { title: string; busy: boolean; error: string; children: React.ReactNode }) {
  const body = useRef<HTMLDivElement>(null);
  const editorChanged = useContext(ServiceEditorContext), clearError = useContext(ServiceCommandErrorContext), recovery=useContext(ServiceRecoveryContext), id = useId();
  return <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy||recovery?.uncertain} error={error}
    onToggle={event => {if(event.currentTarget.open)clearError();editorChanged(id, event.currentTarget.open);}}
    onDiscard={() => body.current?.querySelectorAll("form").forEach(form => form.reset())}>
    <summary>{title}</summary><div ref={body}>{recovery?.notice&&<InlineNotice role="status">{recovery.notice}</InlineNotice>}{recovery?.saved&&<p>Saved original ID: {recovery.saved.id} · <WorkspaceLink href={recovery.saved.href}>{recovery.saved.agreement?"Review saved service agreements":"Open the saved original service order"}</WorkspaceLink></p>}{recovery?.uncertain&&<InlineNotice tone="warning">The original request has an uncertain result. Your newer entries are retained.<Button appearance="standard" type="button" disabled={busy} onClick={()=>void recovery.retry()}>Retry original Service request</Button></InlineNotice>}<fieldset disabled={busy||recovery?.uncertain} style={{display:"contents"}}>{children}</fieldset></div>
  </FormDisclosure>;
}
