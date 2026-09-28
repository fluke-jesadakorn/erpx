"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type {
  productionDetail,
  productionWorkspace,
} from "@/server/production";
import { AttachmentPanel, type AttachmentPanelChange } from "@/components/attachments/attachment-panel";
export function ProductionConsole({
  base,
  module,
  data,
  detail,
}: {
  base: string;
  module: "planning" | "manufacturing";
  data?: Awaited<ReturnType<typeof productionWorkspace>>;
  detail?: Awaited<ReturnType<typeof productionDetail>>;
}) {
  const router = useRouter(),
    last = useRef<{ body: string; key: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body)
      last.current = { body, key: crypto.randomUUID() };
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
      if (j.data.orderId)
        router.push(`${base}/manufacturing/${j.data.orderId}`);
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
      {error && <p role="alert">{error}</p>}
      {data && module === "manufacturing" && (
        <>
          <form
            className="card business-form"
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
                lines: [
                  {
                    productId: f.materialProductId,
                    quantity: f.materialQuantity,
                    substitutionGroup: null,
                  },
                ],
              });
            }}
          >
            <h2>Draft BOM or formula</h2>
            <label>
              Name
              <input name="name" required />
            </label>
            <label>
              Mode
              <select name="mode">
                <option value="bom">BOM assembly</option>
                <option value="formula">Formula batch</option>
              </select>
            </label>
            <label>
              Output
              <select name="outputProductId">
                {data.products.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Batch quantity
              <input name="batchQuantity" required />
            </label>
            <label>
              Expected yield bps
              <input
                name="expectedYieldBps"
                type="number"
                defaultValue="10000"
                required
              />
            </label>
            <label>
              Material
              <select name="materialProductId">
                {data.products.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Material quantity
              <input name="materialQuantity" required />
            </label>
            <button className="button" disabled={busy}>
              Draft specification
            </button>
          </form>
          {data.specifications
            .filter((x) => x.status === "draft")
            .map((x) => (
              <form
                className="card business-form"
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
                <label>
                  Reason
                  <textarea name="reason" required minLength={3} />
                </label>
                <button className="button" disabled={busy}>
                  Approve specification
                </button>
              </form>
            ))}
        </>
      )}
      {data && module === "planning" && (
        <>
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e),
                s = data.specifications.find((x) => x.id === f.specificationId);
              void send({
                ...f,
                entity: "scenario",
                specificationVersion: Number(s?.version),
                capacityMinutes: Number(f.capacityMinutes),
              });
            }}
          >
            <h2>Calculate plan</h2>
            <label>
              Specification
              <select name="specificationId">
                {data.specifications
                  .filter((x) => x.status === "approved")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.name)}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              Warehouse
              <select name="warehouseId">
                {data.warehouses.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Demand quantity
              <input name="demandQuantity" required />
            </label>
            <label>
              Due date
              <input name="dueDate" type="date" required />
            </label>
            <label>
              Capacity minutes
              <input name="capacityMinutes" type="number" min="0" required />
            </label>
            <button className="button" disabled={busy}>
              Calculate scenario
            </button>
          </form>
          {data.recommendations
            .filter((x) => x.status === "proposed")
            .map((x) => (
              <form
                className="card business-form"
                key={String(x.id)}
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
                <h2>
                  Review recommendation · feasible {String(x.feasible_date)}
                </h2>
                <label>
                  Reason
                  <textarea name="reason" required minLength={3} />
                </label>
                <button className="button" disabled={busy}>
                  Approve and release order
                </button>
              </form>
            ))}
        </>
      )}
      {o && <OrderAction key={`${String(o.id)}:${Number(o.version)}`} base={base} order={o} send={send} busy={busy} />}
    </>
  );
}
function OrderAction({
  base,
  order: o,
  send,
  busy,
}: {
  base: string;
  order: Record<string, unknown>;
  send: (i: unknown) => Promise<void>;
  busy: boolean;
}) {
  const [evidence, setEvidence] = useState<AttachmentPanelChange | null>(null);
  const [qcAction, setQcAction] = useState<"pass-qc" | "fail-qc">("pass-qc");
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
  if (status === "in_progress")
    return (
      <form
        className="card business-form"
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
        <label>
          Output quantity
          <input name="outputQuantity" required />
        </label>
        <label>
          Scrap quantity
          <input name="scrapQuantity" required />
        </label>
        <label>
          Labor cost
          <input name="laborCost" required />
        </label>
        <label>
          Overhead cost
          <input name="overheadCost" required />
        </label>
        <label>
          Evidence
          <textarea name="reason" required minLength={3} />
        </label>
        <AttachmentPanel
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
        <button className="button" disabled={busy}>
          Send to QC
        </button>
      </form>
    );
  if (!simple) return null;
  return (
    <form
      className="card business-form"
      onSubmit={(e) => {
        e.preventDefault();
        const action = simple === "pass-qc" ? qcAction : simple;
        void send({
          entity: "order",
          id: o.id,
          action,
          expectedVersion: Number(o.version),
          reason: String(new FormData(e.currentTarget).get("reason")),
          attachmentSetIds:
            evidence?.setId && evidence.readyCount > 0 && evidence.bindable
              ? [evidence.setId]
              : [],
          ...(simple === "reserve" ? { expiresAt: Date.now() + 3600000 } : {}),
        });
      }}
    >
      <h2>{simple.replace("-", " ")}</h2>
      {simple === "pass-qc" && (
        <label>
          QC disposition
          <select value={qcAction} onChange={(event) => setQcAction(event.target.value as "pass-qc" | "fail-qc")}>
            <option value="pass-qc">Pass QC and complete</option>
            <option value="fail-qc">Fail QC and require rework</option>
          </select>
        </label>
      )}
      <label>
        Evidence
        <textarea name="reason" required minLength={3} />
      </label>
      {simple === "pass-qc" && (
        <AttachmentPanel
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
      {simple === "rework" && (
        <AttachmentPanel
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
      <button className="button" disabled={busy}>
        Continue
      </button>
    </form>
  );
}
