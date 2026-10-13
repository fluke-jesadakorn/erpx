"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, Table, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import styles from "@/components/procurement/supplier-promise-console.module.css";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { supplierPromiseDetail } from "@/server/modules/procurement/public/supplier-promises";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { Card, TableFrame } from "@/components/ui/foundation";
type Data = Awaited<ReturnType<typeof supplierPromiseDetail>>;
export function SupplierPromiseConsole({ base, data }: { base: string; data: Data }) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [kind, setKind] = useState("confirmed"),
    [attachments, setAttachments] = useState<AttachmentPanelChange | null>(null),
    retry = useRef<{ fingerprint: string; key: string } | null>(null);
  const [rows, setRows] = useState(() =>
      data.lines
        .filter((l) => l.outstanding_milli > 0)
        .map((l, i) => ({
          key: i,
          lineId: String(l.id),
          quantity: String(l.outstanding_milli / 1000),
          date: "",
        })),
    ),
    serial = useRef(rows.length);
  async function submit(form: FormData) {
    if (busy) return;
    setBusy(true);
    setError("");
    const input = {
      expectedVersion: Number(data.order.version),
      kind,
      confirmationDate: form.get("confirmationDate"),
      supplierReference: form.get("reference"),
      evidence: form.get("evidence"),
      attachmentSetIds:
        attachments?.setId && attachments.readyCount > 0 && attachments.bindable
          ? [attachments.setId]
          : [],
      lines:
        kind === "withdrawn"
          ? []
          : rows.map((r) => ({
              orderLineId: r.lineId,
              quantity: r.quantity,
              promisedDate: r.date,
            })),
    };
    const fingerprint = JSON.stringify(input);
    if (retry.current?.fingerprint !== fingerprint)
      retry.current = { fingerprint, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`/api/v1${base}/supplier-promises/${data.order.id}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": retry.current.key },
        body: JSON.stringify(input),
      });
      const body = await response.json();
      if (!response.ok) {
        if (response.status < 500) retry.current = null;
        throw new Error(body.error?.message ?? "Unable to record supplier confirmation.");
      }
      retry.current = null;
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry to recover the same command result.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel" id="supplier-promises">
      <h2>Supplier delivery confirmations</h2>
      <p>
        The purchase order required date remains {String(data.order.due_date)}. These are supplier
        commitments recorded from evidence; actual warehouse receipts determine the remaining
        quantities.
      </p>
      <TableFrame appearance="standard" label="Records"><Table className="business-table">
        <thead>
          <tr>
            <th>Product</th>
            <th>Still to receive</th>
            <th>Not confirmed by supplier</th>
          </tr>
        </thead>
        <tbody>
          {data.lines.map((l) => (
            <tr key={String(l.id)}>
              <td>
                {String(l.sku)} · {String(l.description)}
              </td>
              <td>{l.outstanding_milli / 1000}</td>
              <td>{l.unconfirmed_milli / 1000}</td>
            </tr>
          ))}
        </tbody>
      </Table></TableFrame>
      <h3>Current promised deliveries</h3>
      {data.open
        .filter((l) => Number(l.remaining_milli) > 0)
        .map((l) => (
          <p key={String(l.id)}>
            {String(l.description)} · {Number(l.remaining_milli) / 1000} remaining ·{" "}
            {String(l.promised_date)}
            {String(l.promised_date) > String(data.order.due_date)
              ? " · Later than the purchase order requirement"
              : ""}
          </p>
        ))}
      {!data.open.some((l) => Number(l.remaining_milli) > 0) && (
        <p>No outstanding supplier-confirmed quantities.</p>
      )}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data.canRecord && (
        <form
          className={`business-form ${styles.form}`}
          onSubmit={(e) => {
            e.preventDefault();
            void submit(new FormData(e.currentTarget));
          }}
        >
          <h3>Record a replacement confirmation</h3>
          <p>
            Enter the complete current commitment for the remaining order. Earlier versions stay in
            history. Quantities not included remain unconfirmed.
          </p>
          <Field appearance="standard">
            Confirmation action
            <Select appearance="standard" value={kind} onChange={(e) => setKind(e.target.value)}>
              <option value="confirmed">Record supplier confirmation</option>
              <option value="withdrawn">Supplier withdrew commitment</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Supplier confirmation date
            <TextInput appearance="standard"
              type="date"
              name="confirmationDate"
              required
              defaultValue={new Date().toISOString().slice(0, 10)}
            />
          </Field>
          <Field appearance="standard">
            Supplier reference
            <TextInput appearance="standard" name="reference" minLength={3} maxLength={160} required />
          </Field>
          <Field appearance="standard">
            Supplier evidence / change reason
            <TextArea appearance="standard" name="evidence" minLength={5} maxLength={2000} required />
          </Field>
          <AttachmentPanel appearance="standard"
            base={base}
            targetType="purchase_order"
            targetId={String(data.order.id)}
            expectedTargetVersion={Number(data.order.version)}
            componentKind="action"
            componentRef="supplier-promise"
            purposeKey="supplier_promise"
            title="Supplier promise evidence"
            description="Attach the supplier confirmation, revised delivery commitment, or withdrawal correspondence reviewed for this promise."
            visibility="Visible to authorized Procurement, receiving, and Finance colleagues in this Company scope."
            onChange={setAttachments}
          />
          {kind === "confirmed" && (
            <>
              {rows.map((r, i) => (
                <fieldset key={r.key}>
                  <legend>Delivery {i + 1}</legend>
                  <Field appearance="standard">
                    Purchase line
                    <Select appearance="standard"
                      value={r.lineId}
                      onChange={(e) =>
                        setRows((prev) =>
                          prev.map((x) => (x.key === r.key ? { ...x, lineId: e.target.value } : x)),
                        )
                      }
                    >
                      {data.lines
                        .filter((l) => l.outstanding_milli > 0)
                        .map((l) => (
                          <option key={String(l.id)} value={String(l.id)}>
                            {String(l.sku)} · {String(l.description)} · {l.outstanding_milli / 1000}{" "}
                            outstanding
                          </option>
                        ))}
                    </Select>
                  </Field>
                  <Field appearance="standard">
                    Promised quantity
                    <TextInput appearance="standard"
                      type="number"
                      step="0.001"
                      min="0.001"
                      required
                      value={r.quantity}
                      onChange={(e) =>
                        setRows((prev) =>
                          prev.map((x) =>
                            x.key === r.key ? { ...x, quantity: e.target.value } : x,
                          ),
                        )
                      }
                    />
                  </Field>
                  <Field appearance="standard">
                    Promised arrival
                    <TextInput appearance="standard"
                      type="date"
                      required
                      value={r.date}
                      onChange={(e) =>
                        setRows((prev) =>
                          prev.map((x) => (x.key === r.key ? { ...x, date: e.target.value } : x)),
                        )
                      }
                    />
                  </Field>
                  <Button appearance="standard"
                    variant="secondary"
                    type="button"
                    onClick={() => setRows((prev) => prev.filter((x) => x.key !== r.key))}
                  >
                    Remove delivery {i + 1}
                  </Button>
                </fieldset>
              ))}
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={rows.length >= 100}
                onClick={() =>
                  setRows((prev) => [
                    ...prev,
                    {
                      key: serial.current++,
                      lineId: String(data.lines.find((l) => l.outstanding_milli > 0)?.id ?? ""),
                      quantity: "",
                      date: "",
                    },
                  ])
                }
              >
                Add split delivery
              </Button>
            </>
          )}
          <Button appearance="standard"
            clientManaged type="submit"
            variant="primary"
            className="primary"
            disabled={busy || (kind === "confirmed" && !rows.length)}
          >
            Record supplier evidence
          </Button>
        </form>
      )}
      <h3>Immutable confirmation history</h3>
      {data.history.length ? (
        data.history.map((h) => (
          <article className={styles.history} key={String(h.id)}>
            <h4>
              {String(h.kind)} · {String(h.confirmation_date)} · {String(h.supplier_reference)}
            </h4>
            <p>{String(h.evidence)}</p>
            <p>
              Recorded against PO version {String(h.order_version)} · {String(h.recorded_by)}
            </p>
            {data.schedule
              .filter((l) => l.promise_id === h.id)
              .map((l) => (
                <p key={String(l.id)}>
                  {String(data.lines.find((s) => s.id === l.order_line_id)?.description)} ·{" "}
                  {Number(l.quantity_milli) / 1000} · {String(l.promised_date)}
                </p>
              ))}
          </article>
        ))
      ) : (
        <p>No supplier evidence recorded.</p>
      )}
    </Card>
  );
}
