"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, Select, Table, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useRef, useState } from "react";
import styles from "@erpx/feature-procurement/components/supplier-promise-console.module.css";
import type { supplierPromiseDetail } from "@erpx/server-procurement/public/supplier-promises";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@erpx/feature-files/components/attachment-panel";
import {useProcurementCommand} from "@erpx/feature-procurement/components/use-procurement-command";
import {ProcurementEditor} from "@erpx/feature-procurement/components/procurement-editor";
import {RecordLink as Link} from "@erpx/feature-records/components/record-link";
import { Card, TableFrame } from "@erpx/ui/components/foundation";
type Data = Awaited<ReturnType<typeof supplierPromiseDetail>>;
export function SupplierPromiseConsole({ base, data }: { base: string; data: Data }) {
  const {busy,error,send,retry,uncertain,notice,savedHref}=useProcurementCommand(base,data.purchase),
    [generation,setGeneration]=useState(0),
    [draftVersion,setDraftVersion]=useState(Number(data.order.version)),
    [kind, setKind] = useState("confirmed"),
    [attachments, setAttachments] = useState<AttachmentPanelChange | null>(null);
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
  async function submit(element:HTMLFormElement) {
    if(busy || !data.canRecord)return;
    const form=new FormData(element),input={expectedVersion:Number(form.get("expectedVersion")),kind,confirmationDate:form.get("confirmationDate"),supplierReference:form.get("reference"),evidence:form.get("evidence"),attachmentSetIds:attachments?.setId&&attachments.readyCount>0&&attachments.bindable?[attachments.setId]:[],lines:kind==="withdrawn"?[]:rows.map(row=>({orderLineId:row.lineId,quantity:row.quantity,promisedDate:row.date}))};
    await send(`supplier-promises/${data.order.id}`,input,false,element);
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
      {notice&&<InlineNotice>{notice} {savedHref&&<Link href={savedHref}>Open saved record</Link>}</InlineNotice>}
      {uncertain&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>void retry()}>Retry submitted purchasing change</Button>}
      <ProcurementEditor title="Record replacement confirmation" onBegin={()=>setDraftVersion(Number(data.order.version))} error={error} busy={busy} disabled={!data.canRecord} onDiscard={()=>{serial.current=data.lines.filter(line=>line.outstanding_milli>0).length;setKind("confirmed");setAttachments(null);setRows(data.lines.filter(line=>line.outstanding_milli>0).map((line,i)=>({key:i,lineId:String(line.id),quantity:String(line.outstanding_milli/1000),date:""})));setGeneration(value=>value+1);}}>
        <form key={generation} data-procurement-draft="promise"
          className={`business-form ${styles.form}`}
          onSubmit={(e) => {
            e.preventDefault();
            void submit(e.currentTarget);
          }}
        >
          <input type="hidden" name="expectedVersion" defaultValue={draftVersion}/><fieldset disabled={!data.canRecord || busy}>
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
                    data-procurement-edit onClick={() => setRows((prev) => prev.filter((x) => x.key !== r.key))}
                  >
                    Remove delivery {i + 1}
                  </Button>
                </fieldset>
              ))}
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={rows.length >= 100}
                data-procurement-edit onClick={() =>
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
          </fieldset>
        </form>
      </ProcurementEditor>
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
