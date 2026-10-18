"use client";
import WorkspaceLink from "@/components/workplace/workspace-link";
import { InlineNotice } from "@/components/ui/foundation";

import {ManufacturingDraftBoundary,ManufacturingDisclosure,ManufacturingForm as FormLayout} from "./draft-boundary";
import {useProductionCommand} from "./use-production-command";
import {useManufacturingOrderCommand} from "./use-order-command";
import type {ManufacturingOrderActor} from "@/lib/manufacturing/order-command";
import {DraftFormDrawer} from "@/components/ui/draft-form-drawer";
import {PlanningRecommendationReviews} from "./planning-recommendation-reviews";
import type {PlanningReviewActor} from "@/lib/manufacturing/planning-review-command";
import type {ManufacturingPurchaseControls} from "@/lib/manufacturing/purchase-controls";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { Children, isValidElement, createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import type { productionDetail, productionWorkspace } from "@/server/modules/manufacturing/public/production";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { Card } from "@/components/ui/foundation";
const ProductionMessages=createContext<{error:string;notice:string;uncertain:boolean;busy:boolean;retry:()=>Promise<void>;refreshSaved?:()=>void}|null>(null);
export function ProductionConsole({ recordOnly=false,
  base,
  module,
  data,
  detail,
  reviewActor,
  orderActor,
}: { recordOnly?:boolean;
  reviewActor?:PlanningReviewActor;
  orderActor?:ManufacturingOrderActor;
  base: string;
  module: "planning" | "manufacturing";
  data?: Awaited<ReturnType<typeof productionWorkspace>>;
  detail?: Awaited<ReturnType<typeof productionDetail>>;
}) {
  const purchase=data&&"purchase" in data?data.purchase:detail&&"purchase" in detail?detail.purchase:undefined,
    recordPurchases=data&&"recordPurchases" in data?data.recordPurchases:undefined,advanced=data&&"advancedPurchase" in data?data.advancedPurchase:undefined,
    orderCommand=useManufacturingOrderCommand(orderActor&&detail?{base,actor:orderActor,order:detail.order,operations:detail.operations,materialLots:detail.materialLots,canWrite:detail.canWriteProduction,canReview:detail.canReviewProduction,purchase}:null),
    command=useProductionCommand(base,purchase,recordPurchases,advanced),{send:sendOriginal,error:commandError,notice,pending:originalBusy,uncertain}=command,
    reviewPendingRef=useRef(false),[reviewRetained,setReviewRetained]=useState(false),[reviewBusy,setReviewBusy]=useState(false),[reviewError,setReviewError]=useState(""),
    [materialCount,setMaterialCount]=useState(1),[operationCount,setOperationCount]=useState(0),[demandSource,setDemandSource]=useState(""),[draftData,setDraftData]=useState(data),editorData=draftData??data;
  const vals = (e: React.FormEvent<HTMLFormElement>) =>
      Object.fromEntries(new FormData(e.currentTarget)),
    o = detail?.order;
  const reviewRetention=useCallback((retained:boolean,working:boolean)=>{reviewPendingRef.current=retained;setReviewRetained(retained);setReviewBusy(working);if(!retained)setReviewError("");},[]),busy=originalBusy||reviewBusy||reviewRetained||orderCommand.pending||orderCommand.uncertain||orderCommand.needsRefresh,error=orderCommand.enabled?(orderCommand.retained()?reviewError:"")||orderCommand.error:reviewError||commandError;
  const send=(input:unknown)=>{if(orderCommand.retained()){setReviewError("Recover the original Manufacturing order change before submitting newer production work. Your draft is retained.");return Promise.resolve();}if(reviewPendingRef.current){setReviewError("Recover the original Planning review before submitting another production change. Your draft is retained.");return Promise.resolve();}return sendOriginal(input);};
  return (
    <ProductionMessages.Provider value={{error,notice:orderCommand.enabled?orderCommand.notice:notice,uncertain:orderCommand.enabled?orderCommand.uncertain:uncertain,busy:orderCommand.enabled?orderCommand.pending:busy,retry:orderCommand.enabled?orderCommand.retry:command.retry,refreshSaved:orderCommand.enabled?orderCommand.refreshSaved:undefined}}><ManufacturingDraftBoundary>
      {(orderCommand.enabled?orderCommand.uncertain:uncertain)&&<InlineNotice tone="warning">The submitted production change has an uncertain response. Retry it with its original values and workspace.<Button appearance="standard" type="button" disabled={orderCommand.enabled?orderCommand.pending:busy} onClick={()=>void (orderCommand.enabled?orderCommand.retry():command.retry())}>Retry submitted change</Button></InlineNotice>}
      {(orderCommand.enabled?orderCommand.notice:notice)&&<InlineNotice tone="info" role="status">{orderCommand.enabled?orderCommand.notice:notice}</InlineNotice>}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {!recordOnly && data && module === "manufacturing" && <ProductionAccounting base={base} data={data} send={send} busy={busy} />}
      {!recordOnly && data && module === "planning" && data.canManageResources && <ProductionResources data={data} send={send} busy={busy} />}
      {data && module === "manufacturing" && (
        <>
          {!recordOnly && <ProductionEditor disabled={!data.canWriteProduction||purchase?.newWork===false} onBeforeOpen={()=>setDraftData(data)} onDiscard={()=>{setMaterialCount(1);setOperationCount(0);}}><summary>Draft BOM or formula</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e);
              void send({
                entity: "specification",
                name: f.name,
                mode: f.mode,
                outputProductId: f.outputProductId,
                batchQuantity: f.batchQuantity,
                expectedYieldBps: Number(f.expectedYieldBps),
                lines: Array.from({ length: materialCount }, (_, n) => ({ productId: f[`materialProductId-${n}`], quantity: f[`materialQuantity-${n}`], substitutionGroup: null })),
                operations: Array.from({ length: operationCount }, (_, n) => ({ resourceId: f[`resource-${n}`], name: f[`operationName-${n}`],
                  setupMinutes: Number(f[`setupMinutes-${n}`]), runMinutesPerBatch: Number(f[`runMinutes-${n}`]) })),
              });
            }}
          >
            <h2>Draft BOM or formula</h2>
            <Field appearance="standard">
              Name
              <TextInput appearance="standard" name="name" required />
            </Field>
            <Field appearance="standard">
              Mode
              <Select appearance="standard" name="mode">
                <option value="bom">BOM assembly</option>
                <option value="formula">Formula batch</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Output
              <Select appearance="standard" name="outputProductId">
                {editorData!.products.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Batch quantity
              <TextInput appearance="standard" name="batchQuantity" required />
            </Field>
            <Field appearance="standard">
              Expected yield bps
              <TextInput appearance="standard" name="expectedYieldBps" type="number" defaultValue="10000" required />
            </Field>
            {Array.from({ length: materialCount }, (_, n) => <fieldset key={`material-${n}`}>
            <legend>Material {n + 1}</legend>
            <Field appearance="standard">
              Product
              <Select appearance="standard" name={`materialProductId-${n}`}>
                {editorData!.products.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Material quantity
              <TextInput appearance="standard" name={`materialQuantity-${n}`} required />
            </Field>
            </fieldset>)}
            <Button appearance="standard" type="button" disabled={busy || materialCount >= 100} onClick={() => setMaterialCount(materialCount + 1)}>Add material</Button>
            {materialCount > 1 && <Button appearance="standard" type="button" disabled={busy} onClick={() => setMaterialCount(materialCount - 1)}>Remove last material</Button>}
            {Array.from({ length: operationCount }, (_, n) => <fieldset key={`operation-${n}`}>
              <legend>Operation {n + 1}</legend>
              <Field appearance="standard">Name<TextInput appearance="standard" name={`operationName-${n}`} required /></Field>
              <Field appearance="standard">Resource<Select appearance="standard" name={`resource-${n}`} required>{editorData!.resources.filter(r => r.status === "active").map(r => <option key={String(r.id)} value={String(r.id)}>{String(r.name)}</option>)}</Select></Field>
              <Field appearance="standard">Setup minutes<TextInput appearance="standard" name={`setupMinutes-${n}`} type="number" min="0" defaultValue="0" required /></Field>
              <Field appearance="standard">Run minutes per batch<TextInput appearance="standard" name={`runMinutes-${n}`} type="number" min="1" required /></Field>
            </fieldset>)}
            <Button appearance="standard" type="button" disabled={busy || !editorData!.resources.some(r => r.status === "active") || operationCount >= 50} onClick={() => setOperationCount(operationCount + 1)}>Add routing operation</Button>
            {operationCount > 0 && <Button appearance="standard" type="button" disabled={busy} onClick={() => setOperationCount(operationCount - 1)}>Remove last operation</Button>}
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || purchase?.newWork===false}>
              Draft specification
            </Button>
          </FormLayout></ProductionEditor>}
          {data.specifications
            .filter((x) => x.status === "draft")
            .map((x) => (
              <ProductionEditor key={String(x.id)} disabled={!data.canReviewProduction||purchase?.newWork===false}><summary>Review {String(x.name)}</summary><FormLayout sourceVersion={Number(x.version)}
                className="ui-panel ui-form-grid"
                onSubmit={(e) => {
                  e.preventDefault();
                  void send({
                    entity: "specification",
                    id: x.id,
                    action: "approve",
                    expectedVersion: Number(x.version),
                    reason: String(new FormData(e.currentTarget).get("reason")),
                  });
                }}
              >
                <h2>Review · {String(x.name)}</h2>
                <Field appearance="standard">
                  Reason
                  <TextArea appearance="standard" name="reason" required minLength={3} />
                </Field>
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || purchase?.newWork===false}>
                  Approve specification
                </Button>
              </FormLayout></ProductionEditor>
            ))}
        </>
      )}
      {data && module === "planning" && (
        <>
          {!recordOnly && <ProductionEditor disabled={!data.canWriteProduction||purchase?.newWork===false} onBeforeOpen={()=>setDraftData(data)} onDiscard={()=>setDemandSource("")}><summary>Calculate plan</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e),
                s = editorData!.specifications.find((x) => x.id === f.specificationId),
                source = editorData!.demandSources.find(x => x.id === demandSource);
              void send({
                ...f,
                entity: "scenario",
                salesOrderId: demandSource || undefined,
                salesOrderVersion: source ? Number(source.version) : undefined,
                demandQuantity: demandSource ? undefined : f.demandQuantity,
                startDate: f.startDate || undefined,
                comparisonScenarioId: f.comparisonScenarioId || undefined,
                specificationVersion: Number(s?.version),
                capacityMinutes: Number(f.capacityMinutes),
                manualMinutesPerUnit: Number(f.manualMinutesPerUnit),
              });
            }}
          >
            <h2>Calculate plan</h2>
            <Field appearance="standard">
              Specification
              <Select appearance="standard" name="specificationId">
                {editorData!.specifications
                  .filter((x) => x.status === "approved")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Warehouse
              <Select appearance="standard" name="warehouseId">
                {editorData!.warehouses.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Demand source
              <Select appearance="standard" value={demandSource} onChange={e => setDemandSource(e.target.value)}>
                <option value="">Manual demand assumption</option>
                {[...new Map(editorData!.demandSources.map(x => [String(x.id), x])).values()].map(x => <option key={String(x.id)} value={String(x.id)}>{String(x.number)} · due {String(x.due_date)}</option>)}
              </Select>
            </Field>
            <Field appearance="standard">
              Demand quantity
              <TextInput appearance="standard" name="demandQuantity" required={!demandSource} disabled={Boolean(demandSource)} />
            </Field>
            <Field appearance="standard">Start date<TextInput appearance="standard" name="startDate" type="date" /></Field>
            <Field appearance="standard">
              Due date
              <TextInput appearance="standard" name="dueDate" type="date" required />
            </Field>
            <Field appearance="standard">
              Capacity minutes
              <TextInput appearance="standard" name="capacityMinutes" type="number" min="0" required />
            </Field>
            <Field appearance="standard">Manual minutes per unit<TextInput appearance="standard" name="manualMinutesPerUnit" type="number" min="1" defaultValue="10" required /></Field>
            <p className="muted">Specifications with routing use the approved resource calendars. Manual minutes apply to specifications without routing.</p>
            <Field appearance="standard">Compare with scenario<Select appearance="standard" name="comparisonScenarioId" disabled={advanced?.newWork===false}><option value="">Start a new plan</option>{editorData!.plans.map(x => <option key={String(x.id)} value={String(x.id)}>{String(x.specification_name)} · due {String(x.due_date)}</option>)}</Select></Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || purchase?.newWork===false}>
              Calculate scenario
            </Button>
          </FormLayout></ProductionEditor>}
          {reviewActor?<PlanningRecommendationReviews base={base} actor={reviewActor} rows={data.recommendations} canReview={data.canReviewProduction} purchase={purchase} recordPurchases={recordPurchases} blocked={originalBusy||uncertain} onRetentionChange={reviewRetention}/>:data.recommendations
            .filter((x) => x.status === "proposed")
            .map((x) => (
              <ProductionEditor key={String(x.id)} disabled={!data.canReviewProduction||(recordPurchases?.[String(x.id)]??purchase)?.newWork===false}><summary>Review recommendation</summary><FormLayout sourceVersion={Number(x.version)}
                className="ui-panel ui-form-grid"
                data-record-id={String(x.id)}
                onSubmit={(e) => {
                  e.preventDefault();
                  void send({
                    entity: "recommendation",
                    id: x.id,
                    action: "approve",
                    expectedVersion: Number(x.version),
                    reason: String(new FormData(e.currentTarget).get("reason")),
                  });
                }}
              >
                <h2>Review recommendation · feasible {String(x.feasible_date)}</h2>
                <Field appearance="standard">
                  Reason
                  <TextArea appearance="standard" name="reason" required minLength={3} />
                </Field>
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || (recordPurchases?.[String(x.id)]??purchase)?.newWork===false}>
                  Approve and release order
                </Button>
              </FormLayout></ProductionEditor>
            ))}
          {data.plans.map(x => {
            const context = JSON.parse(String(x.context_json)) as { comparison?: { shortageDeltaMilli: number; requiredMinutesDelta: number; feasibleDate: string; currentFeasibleDate: string }; capacityBasis?: string };
            return <Card as="section" appearance="standard" className="ui-panel" key={`scenario-${String(x.id)}`}>
              <h2>{String(x.specification_name)} · {String(x.status)}</h2>
              <p>{context.capacityBasis === "approved_routing_resource_calendar" ? "Resource calendar capacity" : "Manual capacity assumption"}</p>
              {context.comparison && <p>Change from compared plan: shortage {context.comparison.shortageDeltaMilli / 1000} units · required time {context.comparison.requiredMinutesDelta} minutes · feasible {context.comparison.feasibleDate} → {context.comparison.currentFeasibleDate}</p>}
              {x.status === "calculated" && <ProductionEditor disabled={!data.canWriteProduction||(recordPurchases?.[String(x.id)]??purchase)?.settleExisting===false}><summary>Cancel unused scenario</summary><FormLayout sourceVersion={Number(x.version)} className="ui-form-grid" onSubmit={e => { e.preventDefault(); void send({ entity: "scenario", id: x.id, action: "cancel", expectedVersion: Number(x.version), reason: String(new FormData(e.currentTarget).get("reason")) }); }}>
                <Field appearance="standard">Cancellation reason<TextArea appearance="standard" name="reason" required minLength={3} /></Field><Button appearance="standard" clientManaged type="submit" disabled={busy}>Cancel unused scenario</Button>
              </FormLayout></ProductionEditor>}
            </Card>;
          })}
        </>
      )}
      {o && (<>
        <OrderAction
          key={String(o.id)}
          base={base}
          order={o}
          purchase={purchase}
          canWrite={detail?.canWriteProduction??false}
          canReview={detail?.canReviewProduction??false}
          operations={detail?.operations ?? []}
          materialLots={detail?.materialLots ?? []}
          orderSend={orderCommand.enabled?orderCommand.send:undefined}
          send={send}
          busy={busy}
        />
        <UnissuedOrderCancellation order={o} purchase={purchase} canWrite={detail?.canWriteProduction??false} send={send} busy={busy} orderSend={orderCommand.enabled?orderCommand.send:undefined} />
      </>)}
    </ManufacturingDraftBoundary></ProductionMessages.Provider>
  );
}
/** Original cancellation closes an unissued run and its retained reservations.
 * It remains a separate settlement action beside Reserve or Issue. */
function UnissuedOrderCancellation({order:currentOrder,purchase,canWrite,send,busy,orderSend}:{order:Record<string,unknown>;purchase?:ManufacturingPurchaseControls;canWrite:boolean;send:(input:unknown)=>Promise<void>;busy:boolean;orderSend?:(input:unknown,form:HTMLFormElement)=>Promise<void>}){
 const [order,setOrder]=useState(currentOrder);
 const unavailable=!canWrite||purchase?.history===false||purchase?.settleExisting===false;
 if(orderSend)return <ClosedUnissuedOrderCancellation order={order} currentOrder={currentOrder} unavailable={unavailable} busy={busy} send={orderSend} capture={()=>setOrder(currentOrder)}/>;
 if(!["planned","reserved"].includes(String(currentOrder.status)))return null;
 return <ProductionEditor disabled={unavailable} onBeforeOpen={()=>setOrder(currentOrder)}><summary>Cancel unissued order</summary><FormLayout sourceVersion={Number(currentOrder.version)} className="ui-panel ui-form-grid" onSubmit={event=>{
  event.preventDefault();if(busy||unavailable)return;const reason=new FormData(event.currentTarget).get("reason");
  void send({entity:"order",id:order.id,action:"cancel",expectedVersion:Number(order.version),reason:String(reason)});
 }}>
  <h2>Cancel unissued order</h2><p>This cancels the unissued order and releases its retained material reservations and capacity.</p>
  <Field appearance="standard">Cancellation reason<TextArea appearance="standard" name="reason" required minLength={3} maxLength={3000} /></Field>
  <Button appearance="standard" clientManaged type="submit" variant="danger" disabled={busy||unavailable}>Cancel unissued order</Button>
 </FormLayout></ProductionEditor>;
}

function ClosedUnissuedOrderCancellation({order,currentOrder,unavailable,busy,send,capture}:{order:Record<string,unknown>;currentOrder:Record<string,unknown>;unavailable:boolean;busy:boolean;send:(input:unknown,form:HTMLFormElement)=>Promise<void>;capture:()=>void}){const [begun,setBegun]=useState(false);if(!begun&&!["planned","reserved"].includes(String(currentOrder.status)))return null;return <ManufacturingOrderEditor disabled={unavailable} onBeforeOpen={()=>{capture();setBegun(true);}} onOpenChange={setBegun} onDiscard={()=>setBegun(false)}><summary>Cancel unissued order</summary><FormLayout sourceVersion={Number(currentOrder.version)} className="ui-panel ui-form-grid" onSubmit={event=>{event.preventDefault();if(busy||unavailable)return;void send({entity:"order",id:order.id,action:"cancel",expectedVersion:Number(order.version),reason:String(new FormData(event.currentTarget).get("reason"))},event.currentTarget);}}>
  <h2>Cancel unissued order</h2><p>This cancels the unissued order and releases its retained material reservations and capacity.</p><Field appearance="standard">Cancellation reason<TextArea appearance="standard" name="reason" required minLength={3} maxLength={3000} /></Field><Button appearance="standard" clientManaged type="submit" variant="danger" disabled={busy||unavailable}>Cancel unissued order</Button>
 </FormLayout></ManufacturingOrderEditor>;
}

function OrderAction({
  base,
  order: currentOrder,
  operations: currentOperations,
  materialLots: currentLots,
  purchase,
  canWrite,canReview,
  send,
  orderSend,
  busy,
}: {
  base: string;
  order: Record<string, unknown>;
  operations: Array<Record<string, unknown>>;
  materialLots: Array<Record<string, unknown>>;
  send: (i: unknown) => Promise<void>;
  orderSend?:(input:unknown,form:HTMLFormElement)=>Promise<void>;
  busy: boolean;
  purchase?:ManufacturingPurchaseControls;
  canWrite:boolean;canReview:boolean;
}) {
  const Editor=orderSend?ManufacturingOrderEditor:ProductionEditor,dispatch=(input:unknown,form:HTMLFormElement)=>orderSend?orderSend(input,form):send(input);
  const [editing,setEditing]=useState(false),[captured,setCaptured]=useState({order:currentOrder,operations:currentOperations,lots:currentLots}),o=editing?captured.order:currentOrder,operations=editing?captured.operations:currentOperations,materialLots=editing?captured.lots:currentLots;
  const editorProps={disabled:!canWrite||purchase?.history===false,onBeforeOpen:()=>setCaptured({order:currentOrder,operations:currentOperations,lots:currentLots}),onOpenChange:setEditing};
  const [evidence, setEvidence] = useState<AttachmentPanelChange | null>(null);
  const [qcAction, setQcAction] = useState<"pass-qc" | "fail-qc">("pass-qc");
  const [failedAction, setFailedAction] = useState<"rework" | "scrap">("rework");
  const draftStamp=JSON.stringify([qcAction,failedAction,evidence?.setId??null,evidence?.readyCount??0,evidence?.bindable??false,evidence?.links.map(link=>[link.id,link.sourceId,link.sourceState,link.linkState])??[]]);
  const status = String(o.status),
    simple =
      status === "planned"
        ? "reserve"
        : status === "reserved"
          ? "issue"
          : status === "awaiting_qc"
            ? "pass-qc"
            : status === "qc_failed"
              ? "rework"
              : null;
  const operation = operations.find(x => !x.completed_at);
  if (status === "in_progress" && operation) return <Editor {...editorProps} onDiscard={()=>setEvidence(null)}><summary>Complete operation</summary><FormLayout sourceVersion={Number(currentOrder.version)} data-manufacturing-order-draft={orderSend?draftStamp:undefined} className="ui-panel ui-form-grid" onSubmit={e => { e.preventDefault(); const f = Object.fromEntries(new FormData(e.currentTarget));
    void dispatch({ entity: "order", id: o.id, action: "complete-operation", expectedVersion: Number(o.version), operationId: operation.id,
      laborMinutes: Number(f.laborMinutes), machineMinutes: Number(f.machineMinutes), reason: f.reason },e.currentTarget); }}>
    <h2>{String(operation.name)}</h2><p>Resource · {String(operation.resource_name)}</p>
    <Field appearance="standard">Actual labor minutes<TextInput appearance="standard" name="laborMinutes" type="number" min="0" required /></Field>
    <Field appearance="standard">Actual machine minutes<TextInput appearance="standard" name="machineMinutes" type="number" min="0" required /></Field>
    <Field appearance="standard">Completion evidence<TextArea appearance="standard" name="reason" required minLength={3} /></Field>
    <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !canWrite || (["pass-qc","fail-qc","scrap"].includes(simple==="pass-qc"?qcAction:simple==="rework"?failedAction:String(simple))&&!canReview) || purchase?.settleExisting===false}>Complete operation</Button>
  </FormLayout></Editor>;
  if (status === "in_progress")
    return (
      <Editor {...editorProps} onDiscard={()=>{setEvidence(null);setQcAction("pass-qc");setFailedAction("rework");}}><summary>Report output</summary><FormLayout sourceVersion={Number(currentOrder.version)} data-manufacturing-order-draft={orderSend?draftStamp:undefined}
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const f = Object.fromEntries(new FormData(e.currentTarget));
          void dispatch({
            ...f,
            entity: "order",
            id: o.id,
            action: "report",
            expectedVersion: Number(o.version),
            attachmentSetIds:
              evidence?.setId && evidence.readyCount > 0 && evidence.bindable
                ? [evidence.setId]
                : [],
          },e.currentTarget);
        }}
      >
        <h2>Report output</h2>
        <Field appearance="standard">
          Output quantity
          <TextInput appearance="standard" name="outputQuantity" required />
        </Field>
        <Field appearance="standard">
          Scrap quantity
          <TextInput appearance="standard" name="scrapQuantity" required />
        </Field>
        <Field appearance="standard">
          Labor cost
          <TextInput appearance="standard" name="laborCost" required />
        </Field>
        <Field appearance="standard">
          Overhead cost
          <TextInput appearance="standard" name="overheadCost" required />
        </Field>
        <Field appearance="standard">
          Evidence
          <TextArea appearance="standard" name="reason" required minLength={3} />
        </Field>
        <AttachmentPanel appearance="standard"
          base={base}
          targetType="production_run"
          targetId={String(o.id)}
          expectedTargetVersion={Number(o.version)}
          componentKind="action"
          componentRef="output-report"
          purposeKey="run_report_evidence"
          title="Run and output evidence"
          description="Attach the batch or machine record, output photos, scrap photos, and other evidence for this reported run."
          visibility="Visible to authorized Manufacturing and independent Quality reviewers."
          onChange={setEvidence}
        />
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !canWrite || (["pass-qc","fail-qc","scrap"].includes(simple==="pass-qc"?qcAction:simple==="rework"?failedAction:String(simple))&&!canReview) || purchase?.settleExisting===false}>
          Send to QC
        </Button>
      </FormLayout></Editor>
    );
  if (!simple) return null;
  return (
    <Editor {...editorProps} onDiscard={()=>{setEvidence(null);setQcAction("pass-qc");setFailedAction("rework");}}><summary>{simple.replace("-", " ")}</summary><FormLayout sourceVersion={Number(currentOrder.version)} data-manufacturing-order-draft={orderSend?draftStamp:undefined}
      className="ui-panel ui-form-grid"
      onSubmit={(e) => {
        e.preventDefault();
        const values = Object.fromEntries(new FormData(e.currentTarget));
        const action = simple === "pass-qc" ? qcAction : simple === "rework" ? failedAction : simple;
        const outputLotAllocations = action !== "pass-qc" || o.output_tracking_mode === "none" ? [] : o.output_tracking_mode === "lot"
          ? [{ quantity: String(Number(o.reported_output_milli) / 1000), lotCode: String(values.outputLotCode ?? ""), expiryDate: values.outputExpiryDate || undefined }]
          : Array.from({ length: Number(o.reported_output_milli) / 1000 }, (_, n) => ({ quantity: "1", serialNumber: String(values[`outputSerial-${n}`] ?? ""), expiryDate: values.outputExpiryDate || undefined }));
        void dispatch({
          entity: "order",
          id: o.id,
          action,
          expectedVersion: Number(o.version),
          reason: String(values.reason),
          entryDate: values.entryDate || undefined,
          ...(action === "issue" ? { materialLotAllocations: materialLots.filter(lot => Number(values[`materialQuantity-${String(lot.id)}`]) > 0)
            .map(lot => ({ productId: lot.product_id, lotId: lot.id, quantity: String(values[`materialQuantity-${String(lot.id)}`]), balanceVersion: Number(lot.balance_version) })) } : {}),
          ...(action === "pass-qc" ? { outputLotAllocations } : {}),
          attachmentSetIds:
            action !== "scrap" && evidence?.setId && evidence.readyCount > 0 && evidence.bindable ? [evidence.setId] : [],
          ...(simple === "reserve" ? { expiresAt: Date.now() + 3600000 } : {}),
        },e.currentTarget);
      }}
    >
      <h2>{simple.replace("-", " ")}</h2>
      {["issue", "pass-qc", "rework"].includes(simple) && <Field appearance="standard">Accounting date<TextInput appearance="standard" name="entryDate" type="date" defaultValue={new Date().toISOString().slice(0, 10)} /></Field>}
      {simple === "issue" && materialLots.map(lot => <Field appearance="standard" key={String(lot.id)}>Issue from {String(lot.lot_code ?? lot.serial_number)} · available {Number(lot.quantity_milli) / 1000}
        <TextInput appearance="standard" name={`materialQuantity-${String(lot.id)}`} defaultValue="0" inputMode="decimal" />
      </Field>)}
      {simple === "pass-qc" && qcAction === "pass-qc" && o.output_tracking_mode === "lot" && <Field appearance="standard">Finished lot code<TextInput appearance="standard" name="outputLotCode" required /></Field>}
      {simple === "pass-qc" && qcAction === "pass-qc" && o.output_tracking_mode === "serial" && Array.from({ length: Number(o.reported_output_milli) / 1000 }, (_, n) => <Field appearance="standard" key={`serial-${n}`}>Finished serial {n + 1}<TextInput appearance="standard" name={`outputSerial-${n}`} required /></Field>)}
      {simple === "pass-qc" && qcAction === "pass-qc" && o.output_tracking_mode !== "none" && <Field appearance="standard">Output expiry date<TextInput appearance="standard" name="outputExpiryDate" type="date" /></Field>}
      {simple === "rework" && <Field appearance="standard">Failed output decision<Select appearance="standard" value={failedAction} onChange={e => setFailedAction(e.target.value as "rework" | "scrap")}>
        <option value="rework">Return to controlled rework</option><option value="scrap">Independently approve complete scrap</option>
      </Select></Field>}
      {simple === "pass-qc" && (
        <Field appearance="standard">
          QC disposition
          <Select appearance="standard"
            value={qcAction}
            onChange={(event) => setQcAction(event.target.value as "pass-qc" | "fail-qc")}
          >
            <option value="pass-qc">Pass QC and complete</option>
            <option value="fail-qc">Fail QC and require rework</option>
          </Select>
        </Field>
      )}
      <Field appearance="standard">
        Evidence
        <TextArea appearance="standard" name="reason" required minLength={3} />
      </Field>
      {simple === "pass-qc" && (
        <AttachmentPanel appearance="standard"
          base={base}
          targetType="production_run"
          targetId={String(o.id)}
          expectedTargetVersion={Number(o.version)}
          componentKind="action"
          componentRef="qc-review"
          purposeKey="qc_evidence"
          title="QC evidence"
          description="Attach the reviewed QC report, certificate, measurements, or inspection photos for this decision."
          visibility="Visible to authorized Manufacturing and independent Quality reviewers."
          onChange={setEvidence}
        />
      )}
      {simple === "rework" && failedAction === "rework" && (
        <AttachmentPanel appearance="standard"
          base={base}
          targetType="production_run"
          targetId={String(o.id)}
          expectedTargetVersion={Number(o.version)}
          componentKind="action"
          componentRef="rework"
          purposeKey="rework_action_evidence"
          title="Rework evidence"
          description="Attach the approved rework instruction, defect evidence, or completion support for this rework cycle."
          visibility="Visible to authorized Manufacturing and independent Quality reviewers."
          onChange={setEvidence}
        />
      )}
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !canWrite || (["pass-qc","fail-qc","scrap"].includes(simple==="pass-qc"?qcAction:simple==="rework"?failedAction:String(simple))&&!canReview) || (status==="planned"?purchase?.newWork:purchase?.settleExisting)===false}>
        Continue
      </Button>
    </FormLayout></Editor>
  );
}

function ProductionAccounting({ base, data, send, busy }: { base:string; data: Awaited<ReturnType<typeof productionWorkspace>>; send: (input: unknown) => Promise<void>; busy: boolean }) {
  if (!data.canConfigureAccounting && !data.canReviewAccounting) return null;
  const purchase="purchase" in data?data.purchase:undefined;
  const categories: Record<string, string[]> = { inventoryAccountId: ["asset"], wipAccountId: ["asset"], laborAccountId: ["liability", "expense"], overheadAccountId: ["liability", "expense"], scrapAccountId: ["expense", "other_expense"] };
  const labels: Record<string, string> = { inventoryAccountId: "Inventory", wipAccountId: "Work in progress", laborAccountId: "Labor clearing", overheadAccountId: "Overhead clearing", scrapAccountId: "Abnormal scrap expense" };
  return <>
    <p><WorkspaceLink href={`${base}/production/manufacturing/settings/posting`}>Review Company posting setup before drafting production accounts</WorkspaceLink></p>
    {data.canConfigureAccounting && !data.accountingProfiles.some(p => p.status === "approved") && <ProductionEditor disabled={purchase?.newWork===false}><summary>Draft production account map</summary><FormLayout className="ui-panel ui-form-grid" onSubmit={e => { e.preventDefault(); const values = Object.fromEntries(new FormData(e.currentTarget));
      void send({ ...values, entity: "accounting", chartId: String(data.accounts[0]?.chart_id ?? "") }); }}>
      <h2>Production accounting</h2><p>Finance approves the accounts used to move materials into WIP, record conversion costs and release inspected output.</p>
      {Object.entries(categories).map(([name, allowed]) => <Field appearance="standard" key={name}>{labels[name]}<Select appearance="standard" name={name} required>{data.accounts.filter(a => allowed.includes(String(a.category))).map(a => <option key={String(a.id)} value={String(a.id)}>{String(a.code)} · {String(a.name)}</option>)}</Select></Field>)}
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !data.accounts.length}>Draft account map</Button>
    </FormLayout></ProductionEditor>}
    {data.canReviewAccounting && data.accountingProfiles.filter(p => p.status === "draft").map(p => <ProductionEditor key={String(p.id)} disabled={purchase?.newWork===false}><summary>Review production account map</summary><FormLayout sourceVersion={Number(p.version)} className="ui-panel ui-form-grid" onSubmit={e => { e.preventDefault(); void send({ entity: "accounting", id: p.id, action: "approve", expectedVersion: Number(p.version), reason: String(new FormData(e.currentTarget).get("reason")) }); }}>
      <h2>Independently review production accounts</h2><dl>{Object.keys(categories).map(name => { const column = name.replace(/[A-Z]/g, c => `_${c.toLowerCase()}`), account = data.accounts.find(a => a.id === p[column]);
        return <div key={name}><dt>{labels[name]}</dt><dd>{String(account?.name ?? p[column])}</dd></div>; })}</dl>
      <Field appearance="standard">Review reason<TextArea appearance="standard" name="reason" required minLength={3} /></Field><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Approve account map</Button>
    </FormLayout></ProductionEditor>)}
  </>;
}

function ProductionResources({ data, send, busy }: { data: Awaited<ReturnType<typeof productionWorkspace>>; send: (input: unknown) => Promise<void>; busy: boolean }) {
  const purchase="purchase" in data?data.purchase:undefined;
  const branches = [...new Set(data.warehouses.map(w => String(w.branch_id)))];
  return <>
    <ProductionEditor disabled={purchase?.newWork===false}><summary>Create production resource</summary><FormLayout className="ui-panel ui-form-grid" onSubmit={e => { e.preventDefault(); const f = Object.fromEntries(new FormData(e.currentTarget));
      void send({ ...f, entity: "resource", dailyMinutes: Number(f.dailyMinutes), workWeekdays: [1, 2, 3, 4, 5] }); }}>
      <h2>Production resources</h2><Field appearance="standard">Branch<Select appearance="standard" name="branchId" required>{branches.map(id => <option key={id} value={id}>{String(data.warehouses.find(w => w.branch_id === id)?.name ?? id)} branch</option>)}</Select></Field>
      <Field appearance="standard">Resource code<TextInput appearance="standard" name="code" required minLength={2} /></Field><Field appearance="standard">Name<TextInput appearance="standard" name="name" required minLength={2} /></Field>
      <Field appearance="standard">Available minutes per weekday<TextInput appearance="standard" name="dailyMinutes" type="number" min="1" max="1440" required /></Field>
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !branches.length}>Create resource</Button>
    </FormLayout></ProductionEditor>
    {data.resources.filter(r => r.status === "active").map(resource => <ProductionCalendar key={String(resource.id)} resource={resource} calendar={data.calendar} send={send} busy={busy} purchase={purchase} />)}
  </>;
}

function ProductionCalendar({ resource, calendar, send, busy, purchase }: {purchase?:ManufacturingPurchaseControls; resource: Record<string, unknown>; calendar: Array<Record<string, unknown>>; send: (input: unknown) => Promise<void>; busy: boolean }) {
  const today=()=>new Date().toISOString().slice(0,10),[workDate,setWorkDate]=useState(today),[capturedCalendar,setCapturedCalendar]=useState(calendar),[resourceVersion,setResourceVersion]=useState(Number(resource.version)),current=capturedCalendar.find(c=>c.resource_id===resource.id&&c.work_date===workDate),live=calendar.find(c=>c.resource_id===resource.id&&c.work_date===workDate),stale=Number(current?.version??0)!==Number(live?.version??0);
  return <ProductionEditor disabled={purchase?.newWork===false} onBeforeOpen={()=>{setCapturedCalendar(calendar);setResourceVersion(Number(resource.version));}} onDiscard={()=>setWorkDate(today())}><summary>{String(resource.name)} calendar</summary><FormLayout sourceVersion={Number(resource.version)} className="ui-panel ui-form-grid" onSubmit={e=>{e.preventDefault();if(stale)return;const f=Object.fromEntries(new FormData(e.currentTarget));
    void send({entity:"resource-calendar",id:resource.id,resourceVersion,workDate,expectedVersion:Number(current?.version??0),availableMinutes:Number(f.availableMinutes),reason:f.reason});}}>
    {stale&&<InlineNotice tone="warning">This calendar date changed. Your draft is retained; reopen with its current revision before submitting.</InlineNotice>}
    <h2>{String(resource.name)} calendar</h2><p>{String(resource.daily_minutes)} minutes on regular working days</p>
    <Field appearance="standard">Date<TextInput appearance="standard" type="date" value={workDate} onChange={e => setWorkDate(e.target.value)} required /></Field>
    <Field appearance="standard">Available minutes<TextInput appearance="standard" key={`${workDate}:${String(current?.version ?? 0)}`} name="availableMinutes" type="number" min="0" max="1440" defaultValue={String(current?.available_minutes ?? resource.daily_minutes)} required /></Field>
    <Field appearance="standard">Change reason<TextArea appearance="standard" name="reason" required minLength={3} /></Field><Button appearance="standard" clientManaged type="submit" disabled={busy || stale || purchase?.newWork===false}>Save capacity exception</Button>
  </FormLayout></ProductionEditor>;
}

function ProductionEditor({children,onDiscard=()=>{},onBeforeOpen,onOpenChange,disabled}:{children:ReactNode;onDiscard?:()=>void;onBeforeOpen?:()=>void;onOpenChange?:(open:boolean)=>void;disabled?:boolean}){
 const messages=useContext(ProductionMessages);
 return <ManufacturingDisclosure disabled={disabled} error={messages?.error} onDiscard={onDiscard} onBeforeOpen={onBeforeOpen} onOpenChange={onOpenChange}>{messages?.notice&&<InlineNotice tone="info" role="status">{messages.notice}</InlineNotice>}{messages?.uncertain&&<InlineNotice tone="warning">Resolve the submitted change before sending newer edits.<Button appearance="standard" type="button" disabled={messages.busy} onClick={()=>void messages.retry()}>Retry submitted change</Button></InlineNotice>}{children}</ManufacturingDisclosure>;
}

function ManufacturingOrderEditor({children,onDiscard=()=>{},onBeforeOpen,onOpenChange,disabled}:{children:ReactNode;onDiscard?:()=>void;onBeforeOpen?:()=>void;onOpenChange?:(open:boolean)=>void;disabled?:boolean}){
 const messages=useContext(ProductionMessages),root=useRef<HTMLDivElement>(null),[dirty,setDirty]=useState(false),[open,setOpen]=useState(false),[begun,setBegun]=useState(false),[instance,setInstance]=useState(0);
 const nodes=Children.toArray(children),summary=nodes.find(node=>isValidElement(node)&&node.type==='summary'),title=(node:ReactNode):string=>typeof node==='string'||typeof node==='number'?String(node):isValidElement<{children?:ReactNode}>(node)?title(node.props.children):Children.toArray(node).map(title).join(''),label=isValidElement<{children?:ReactNode}>(summary)?summary.props.children:'Manufacturing order change';
 useEffect(()=>{const accepted=(event:Event)=>{const completion=(event as CustomEvent<{form?:HTMLFormElement;formEditVersion?:string|null}>).detail;if(completion?.form&&root.current?.contains(completion.form)&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion){setDirty(false);setOpen(false);setBegun(false);onDiscard();onOpenChange?.(false);}};window.addEventListener('erpx:manufacturing-command-accepted',accepted);return()=>window.removeEventListener('erpx:manufacturing-command-accepted',accepted);},[onOpenChange,onDiscard]);
 const edited=()=>{setDirty(true);},close=()=>{if(messages?.busy)return;setOpen(false);},discard=()=>{if(messages?.uncertain||messages?.busy){close();return;}const editor=root.current;if(editor){for(const form of editor.querySelectorAll<HTMLFormElement>('form[data-manufacturing-draft]'))form.reset();window.dispatchEvent(new CustomEvent('erpx:record-saved',{detail:{root:editor}}));}setDirty(false);setBegun(false);onDiscard();onOpenChange?.(false);close();messages?.refreshSaved?.();};
 return <><Button appearance="standard" type="button" variant="secondary" disabled={disabled&&!begun||messages?.busy} onClick={()=>{if(messages?.busy||disabled&&!begun)return;if(!begun){onBeforeOpen?.();setInstance(value=>value+1);setBegun(true);}setOpen(true);onOpenChange?.(true);}}>{begun?'Resume retained '+title(label):label}</Button>{begun&&<DraftFormDrawer appearance="standard" retainFormContents open={open} title={title(label)} dirty={dirty&&!messages?.uncertain} dismissDisabled={messages?.busy} onClose={close} onDiscard={discard}>
  {messages?.error&&<InlineNotice tone="danger" role="alert">{messages.error}</InlineNotice>}{messages?.notice&&<InlineNotice tone="info" role="status">{messages.notice}</InlineNotice>}
  {messages?.uncertain&&<InlineNotice tone="warning">Recover the original Manufacturing change before sending newer fields.<Button appearance="standard" type="button" disabled={messages.busy} onClick={()=>void messages.retry()}>Retry submitted change</Button></InlineNotice>}
  <div key={instance} ref={root} onInputCapture={edited} onChangeCapture={edited} onClickCapture={event=>{if(event.target instanceof Element&&event.target.closest('button[type="button"]'))edited();}}>{nodes.filter(node=>node!==summary)}</div>
  <div className="ui-form-actions"><Button appearance="standard" type="button" variant="secondary" disabled={messages?.busy} onClick={close}>Close and retain draft</Button>{!messages?.uncertain&&<Button appearance="standard" type="button" variant="secondary" disabled={messages?.busy} onClick={discard}>Discard this order draft</Button>}</div>
 </DraftFormDrawer>}</>;
}
