"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { productionDetail, productionWorkspace } from "@/server/production";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import { FormLayout, Card } from "@/components/ui/foundation";
export function ProductionConsole({ recordOnly=false,
  base,
  module,
  data,
  detail,
}: { recordOnly?:boolean;
  base: string;
  module: "planning" | "manufacturing";
  data?: Awaited<ReturnType<typeof productionWorkspace>>;
  detail?: Awaited<ReturnType<typeof productionDetail>>;
}) {
  const router = useRouter(),
    last = useRef<{ body: string; key: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [materialCount, setMaterialCount] = useState(1),
    [operationCount, setOperationCount] = useState(0),
    [demandSource, setDemandSource] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/production`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": last.current.key,
          },
          body,
        }),
        j = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(j.error?.message ?? "Unable to update production.");
      }
      last.current = null;
      if (j.data.orderId) router.push(`${base}/manufacturing/${j.data.orderId}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  const vals = (e: React.FormEvent<HTMLFormElement>) =>
      Object.fromEntries(new FormData(e.currentTarget)),
    o = detail?.order;
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {!recordOnly && data && module === "manufacturing" && <ProductionAccounting data={data} send={send} busy={busy} />}
      {!recordOnly && data && module === "planning" && data.canManageResources && <ProductionResources data={data} send={send} busy={busy} />}
      {data && module === "manufacturing" && (
        <>
          {!recordOnly && <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Draft BOM or formula</summary><FormLayout
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
                {data.products.map((x) => (
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
                {data.products.map((x) => (
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
              <Field appearance="standard">Resource<Select appearance="standard" name={`resource-${n}`} required>{data.resources.filter(r => r.status === "active").map(r => <option key={String(r.id)} value={String(r.id)}>{String(r.name)}</option>)}</Select></Field>
              <Field appearance="standard">Setup minutes<TextInput appearance="standard" name={`setupMinutes-${n}`} type="number" min="0" defaultValue="0" required /></Field>
              <Field appearance="standard">Run minutes per batch<TextInput appearance="standard" name={`runMinutes-${n}`} type="number" min="1" required /></Field>
            </fieldset>)}
            <Button appearance="standard" type="button" disabled={busy || !data.resources.some(r => r.status === "active") || operationCount >= 50} onClick={() => setOperationCount(operationCount + 1)}>Add routing operation</Button>
            {operationCount > 0 && <Button appearance="standard" type="button" disabled={busy} onClick={() => setOperationCount(operationCount - 1)}>Remove last operation</Button>}
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Draft specification
            </Button>
          </FormLayout></FormDisclosure>}
          {data.specifications
            .filter((x) => x.status === "draft")
            .map((x) => (
              <FormLayout
                className="ui-panel ui-form-grid"
                key={String(x.id)}
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
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Approve specification
                </Button>
              </FormLayout>
            ))}
        </>
      )}
      {data && module === "planning" && (
        <>
          {!recordOnly && <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Calculate plan</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e),
                s = data.specifications.find((x) => x.id === f.specificationId),
                source = data.demandSources.find(x => x.id === demandSource);
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
                {data.specifications
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
                {data.warehouses.map((x) => (
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
                {[...new Map(data.demandSources.map(x => [String(x.id), x])).values()].map(x => <option key={String(x.id)} value={String(x.id)}>{String(x.number)} · due {String(x.due_date)}</option>)}
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
            <Field appearance="standard">Compare with scenario<Select appearance="standard" name="comparisonScenarioId"><option value="">Start a new plan</option>{data.plans.map(x => <option key={String(x.id)} value={String(x.id)}>{String(x.specification_name)} · due {String(x.due_date)}</option>)}</Select></Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Calculate scenario
            </Button>
          </FormLayout></FormDisclosure>}
          {data.recommendations
            .filter((x) => x.status === "proposed")
            .map((x) => (
              <FormLayout
                className="ui-panel ui-form-grid"
                key={String(x.id)}
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
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Approve and release order
                </Button>
              </FormLayout>
            ))}
          {data.plans.map(x => {
            const context = JSON.parse(String(x.context_json)) as { comparison?: { shortageDeltaMilli: number; requiredMinutesDelta: number; feasibleDate: string; currentFeasibleDate: string }; capacityBasis?: string };
            return <Card as="section" appearance="standard" className="ui-panel" key={`scenario-${String(x.id)}`}>
              <h2>{String(x.specification_name)} · {String(x.status)}</h2>
              <p>{context.capacityBasis === "approved_routing_resource_calendar" ? "Resource calendar capacity" : "Manual capacity assumption"}</p>
              {context.comparison && <p>Change from compared plan: shortage {context.comparison.shortageDeltaMilli / 1000} units · required time {context.comparison.requiredMinutesDelta} minutes · feasible {context.comparison.feasibleDate} → {context.comparison.currentFeasibleDate}</p>}
              {x.status === "calculated" && <FormLayout className="ui-form-grid" onSubmit={e => { e.preventDefault(); void send({ entity: "scenario", id: x.id, action: "cancel", expectedVersion: Number(x.version), reason: String(new FormData(e.currentTarget).get("reason")) }); }}>
                <Field appearance="standard">Cancellation reason<TextArea appearance="standard" name="reason" required minLength={3} /></Field><Button appearance="standard" clientManaged type="submit" disabled={busy}>Cancel unused scenario</Button>
              </FormLayout>}
            </Card>;
          })}
        </>
      )}
      {o && (
        <OrderAction
          key={`${String(o.id)}:${Number(o.version)}`}
          base={base}
          order={o}
          operations={detail?.operations ?? []}
          materialLots={detail?.materialLots ?? []}
          send={send}
          busy={busy}
        />
      )}
    </>
  );
}
function OrderAction({
  base,
  order: o,
  operations,
  materialLots,
  send,
  busy,
}: {
  base: string;
  order: Record<string, unknown>;
  operations: Array<Record<string, unknown>>;
  materialLots: Array<Record<string, unknown>>;
  send: (i: unknown) => Promise<void>;
  busy: boolean;
}) {
  const [evidence, setEvidence] = useState<AttachmentPanelChange | null>(null);
  const [qcAction, setQcAction] = useState<"pass-qc" | "fail-qc">("pass-qc");
  const [failedAction, setFailedAction] = useState<"rework" | "scrap">("rework");
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
  if (status === "in_progress" && operation) return <FormLayout className="ui-panel ui-form-grid" onSubmit={e => { e.preventDefault(); const f = Object.fromEntries(new FormData(e.currentTarget));
    void send({ entity: "order", id: o.id, action: "complete-operation", expectedVersion: Number(o.version), operationId: operation.id,
      laborMinutes: Number(f.laborMinutes), machineMinutes: Number(f.machineMinutes), reason: f.reason }); }}>
    <h2>{String(operation.name)}</h2><p>Resource · {String(operation.resource_name)}</p>
    <Field appearance="standard">Actual labor minutes<TextInput appearance="standard" name="laborMinutes" type="number" min="0" required /></Field>
    <Field appearance="standard">Actual machine minutes<TextInput appearance="standard" name="machineMinutes" type="number" min="0" required /></Field>
    <Field appearance="standard">Completion evidence<TextArea appearance="standard" name="reason" required minLength={3} /></Field>
    <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Complete operation</Button>
  </FormLayout>;
  if (status === "in_progress")
    return (
      <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Report output</summary><FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const f = Object.fromEntries(new FormData(e.currentTarget));
          void send({
            ...f,
            entity: "order",
            id: o.id,
            action: "report",
            expectedVersion: Number(o.version),
            attachmentSetIds:
              evidence?.setId && evidence.readyCount > 0 && evidence.bindable
                ? [evidence.setId]
                : [],
          });
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
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
          Send to QC
        </Button>
      </FormLayout></FormDisclosure>
    );
  if (!simple) return null;
  return (
    <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>{simple.replace("-", " ")}</summary><FormLayout
      className="ui-panel ui-form-grid"
      onSubmit={(e) => {
        e.preventDefault();
        const values = Object.fromEntries(new FormData(e.currentTarget));
        const action = simple === "pass-qc" ? qcAction : simple === "rework" ? failedAction : simple;
        const outputLotAllocations = action !== "pass-qc" || o.output_tracking_mode === "none" ? [] : o.output_tracking_mode === "lot"
          ? [{ quantity: String(Number(o.reported_output_milli) / 1000), lotCode: String(values.outputLotCode ?? ""), expiryDate: values.outputExpiryDate || undefined }]
          : Array.from({ length: Number(o.reported_output_milli) / 1000 }, (_, n) => ({ quantity: "1", serialNumber: String(values[`outputSerial-${n}`] ?? ""), expiryDate: values.outputExpiryDate || undefined }));
        void send({
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
        });
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
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
        Continue
      </Button>
    </FormLayout></FormDisclosure>
  );
}

function ProductionAccounting({ data, send, busy }: { data: Awaited<ReturnType<typeof productionWorkspace>>; send: (input: unknown) => Promise<void>; busy: boolean }) {
  if (!data.canConfigureAccounting && !data.canReviewAccounting) return null;
  const categories: Record<string, string[]> = { inventoryAccountId: ["asset"], wipAccountId: ["asset"], laborAccountId: ["liability", "expense"], overheadAccountId: ["liability", "expense"], scrapAccountId: ["expense", "other_expense"] };
  const labels: Record<string, string> = { inventoryAccountId: "Inventory", wipAccountId: "Work in progress", laborAccountId: "Labor clearing", overheadAccountId: "Overhead clearing", scrapAccountId: "Abnormal scrap expense" };
  return <>
    {data.canConfigureAccounting && !data.accountingProfiles.some(p => p.status === "approved") && <FormLayout className="ui-panel ui-form-grid" onSubmit={e => { e.preventDefault(); const values = Object.fromEntries(new FormData(e.currentTarget));
      void send({ ...values, entity: "accounting", chartId: String(data.accounts[0]?.chart_id ?? "") }); }}>
      <h2>Production accounting</h2><p>Finance approves the accounts used to move materials into WIP, record conversion costs and release inspected output.</p>
      {Object.entries(categories).map(([name, allowed]) => <Field appearance="standard" key={name}>{labels[name]}<Select appearance="standard" name={name} required>{data.accounts.filter(a => allowed.includes(String(a.category))).map(a => <option key={String(a.id)} value={String(a.id)}>{String(a.code)} · {String(a.name)}</option>)}</Select></Field>)}
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !data.accounts.length}>Draft account map</Button>
    </FormLayout>}
    {data.canReviewAccounting && data.accountingProfiles.filter(p => p.status === "draft").map(p => <FormLayout className="ui-panel ui-form-grid" key={String(p.id)} onSubmit={e => { e.preventDefault(); void send({ entity: "accounting", id: p.id, action: "approve", expectedVersion: Number(p.version), reason: String(new FormData(e.currentTarget).get("reason")) }); }}>
      <h2>Independently review production accounts</h2><dl>{Object.keys(categories).map(name => { const column = name.replace(/[A-Z]/g, c => `_${c.toLowerCase()}`), account = data.accounts.find(a => a.id === p[column]);
        return <div key={name}><dt>{labels[name]}</dt><dd>{String(account?.name ?? p[column])}</dd></div>; })}</dl>
      <Field appearance="standard">Review reason<TextArea appearance="standard" name="reason" required minLength={3} /></Field><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Approve account map</Button>
    </FormLayout>)}
  </>;
}

function ProductionResources({ data, send, busy }: { data: Awaited<ReturnType<typeof productionWorkspace>>; send: (input: unknown) => Promise<void>; busy: boolean }) {
  const branches = [...new Set(data.warehouses.map(w => String(w.branch_id)))];
  return <>
    <FormLayout className="ui-panel ui-form-grid" onSubmit={e => { e.preventDefault(); const f = Object.fromEntries(new FormData(e.currentTarget));
      void send({ ...f, entity: "resource", dailyMinutes: Number(f.dailyMinutes), workWeekdays: [1, 2, 3, 4, 5] }); }}>
      <h2>Production resources</h2><Field appearance="standard">Branch<Select appearance="standard" name="branchId" required>{branches.map(id => <option key={id} value={id}>{String(data.warehouses.find(w => w.branch_id === id)?.name ?? id)} branch</option>)}</Select></Field>
      <Field appearance="standard">Resource code<TextInput appearance="standard" name="code" required minLength={2} /></Field><Field appearance="standard">Name<TextInput appearance="standard" name="name" required minLength={2} /></Field>
      <Field appearance="standard">Available minutes per weekday<TextInput appearance="standard" name="dailyMinutes" type="number" min="1" max="1440" required /></Field>
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !branches.length}>Create resource</Button>
    </FormLayout>
    {data.resources.filter(r => r.status === "active").map(resource => <ProductionCalendar key={`${String(resource.id)}:${String(resource.version)}`} resource={resource} calendar={data.calendar} send={send} busy={busy} />)}
  </>;
}

function ProductionCalendar({ resource, calendar, send, busy }: { resource: Record<string, unknown>; calendar: Array<Record<string, unknown>>; send: (input: unknown) => Promise<void>; busy: boolean }) {
  const [workDate, setWorkDate] = useState(new Date().toISOString().slice(0, 10)), current = calendar.find(c => c.resource_id === resource.id && c.work_date === workDate);
  return <FormLayout className="ui-panel ui-form-grid" onSubmit={e => { e.preventDefault(); const f = Object.fromEntries(new FormData(e.currentTarget));
    void send({ entity: "resource-calendar", id: resource.id, resourceVersion: Number(resource.version), workDate,
      expectedVersion: Number(current?.version ?? 0), availableMinutes: Number(f.availableMinutes), reason: f.reason }); }}>
    <h2>{String(resource.name)} calendar</h2><p>{String(resource.daily_minutes)} minutes on regular working days</p>
    <Field appearance="standard">Date<TextInput appearance="standard" type="date" value={workDate} onChange={e => setWorkDate(e.target.value)} required /></Field>
    <Field appearance="standard">Available minutes<TextInput appearance="standard" key={`${workDate}:${String(current?.version ?? 0)}`} name="availableMinutes" type="number" min="0" max="1440" defaultValue={String(current?.available_minutes ?? resource.daily_minutes)} required /></Field>
    <Field appearance="standard">Change reason<TextArea appearance="standard" name="reason" required minLength={3} /></Field><Button appearance="standard" clientManaged type="submit" disabled={busy}>Save capacity exception</Button>
  </FormLayout>;
}
