"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useState,type ComponentPropsWithRef,type FormEvent } from "react";
import {useTransferCommand} from "./use-transfer-command";
import {TransferDisclosure,TransferForm} from "./transfer-draft-boundary";
import type { transitDetail } from "@/server/modules/inventory/public/inventory-transit";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { Stack, Card } from "@/components/ui/foundation";
function TransitForm({title,version,onDiscard,onCommand,pending,children,...props}:Omit<ComponentPropsWithRef<"form">,"onSubmit">&{title:string;version:number;onDiscard:()=>void;pending:boolean;onCommand:(event:FormEvent<HTMLFormElement>)=>Promise<boolean>}){
 const [open,setOpen]=useState(false),[sourceVersion,setSourceVersion]=useState(version),[generation,setGeneration]=useState(0);
 const changed=sourceVersion!==version;
 return <TransferDisclosure dismissDisabled={pending} drawerTitle={title} open={open} onOpenChange={setOpen} onBeforeOpen={()=>setSourceVersion(version)} onDiscard={()=>{onDiscard();setGeneration(value=>value+1);}}><summary>{title}</summary>
  {changed&&<p role="status">The transport changed while this draft was open. Your draft is retained. Review the current record before making another change.</p>}
  <TransferForm {...props} key={generation} onSubmit={event=>{event.preventDefault();if(changed)return;void onCommand(event).then(saved=>{if(saved){setOpen(false);onDiscard();setGeneration(value=>value+1);}});}}><input type="hidden" name="sourceVersion" value={sourceVersion}/><fieldset disabled={changed} className="ui-stack">{children}</fieldset></TransferForm>
 </TransferDisclosure>;
}
export function TransitConsole({
  base,
  data,
}: {
  base: string;
  data: Awaited<ReturnType<typeof transitDetail>>;
}) {
  const receivableLines = data.lines.filter(
      (l) =>
        Number(l.quantity_milli) >
        Number(l.received_milli) + Number(l.lost_milli) + Number(l.pending_loss_milli),
    ),
    command=useTransferCommand(base,"stock-transit"),
    {pending,error,notice,uncertain,retry,canPrepare,canSettle,purchase}=command,
    d = data.transit,
    [lineId, setLineId] = useState(String(receivableLines[0]?.id ?? "")),
    [kind, setKind] = useState<"receive" | "loss">("receive"),
    [lotQuantities, setLotQuantities] = useState<Record<string, string>>({}),
    [attachmentSets, setAttachmentSets] = useState<Record<string, AttachmentPanelChange>>({});
  const send=(action:string,input:unknown)=>command.send(`/${d.id}/${action}`,input);
  const resetReceiving=()=>{setLineId(String(receivableLines[0]?.id??""));setKind("receive");setLotQuantities({});setAttachmentSets({});};
  const expectedVersion = Number(d.version),
    date = () => (
      <Field appearance="standard">
        Business date
        <TextInput appearance="standard"
          name="date"
          type="date"
          required
          min={String(d.dispatch_date ?? "")}
          defaultValue={new Date().toISOString().slice(0, 10)}
        />
      </Field>
    ),
    proof = () => (
      <Field appearance="standard">
        Physical evidence / reference
        <TextArea appearance="standard" name="evidence" required minLength={5} maxLength={2000} />
      </Field>
    );
  const selectedLine = data.lines.find((l) => String(l.id) === lineId),
    selectedLots = data.lots.filter(
      (l) =>
        String(l.transit_line_id) === lineId &&
        Number(l.quantity_milli) >
          Number(l.received_milli) + Number(l.lost_milli) + Number(l.pending_loss_milli),
    ),
    tracked = selectedLots.length > 0,
    selectedQuantity = selectedLots.reduce(
      (sum, l) => sum + (Number(lotQuantities[String(l.id)]) || 0),
      0,
    );
  const setIds = (key: string) => {
    const value = attachmentSets[key];
    return value?.setId && value.readyCount > 0 && value.bindable ? [value.setId] : [];
  };
  return (
    <Stack as="div" className="ui-stack">
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {notice&&<p role="status">{notice}</p>}{uncertain&&<Button appearance="standard" type="button" variant="secondary" disabled={pending} onClick={()=>void retry()}>Retry submitted transport change</Button>}
      {data.canDispatch && (canPrepare || canSettle) && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Record source dispatch</h2>
          <p>
            Confirm the goods have left the source. Quantities and cost must still match the
            approved document. If stock changed, cancel this unshipped transport and prepare a new
            transfer for review.
          </p>
          <TransitForm title="Record source dispatch" version={expectedVersion} pending={pending} onDiscard={()=>setAttachmentSets({})}
            className="ui-form-grid"
            onCommand={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              return send("dispatch", {
                expectedVersion:Number(f.get("sourceVersion")),
                attachmentSetIds: setIds("dispatch"),
                date: f.get("date"),
                trackingReference: f.get("tracking"),
                evidence: f.get("evidence"),
              });
            }}
          >
            {date()}
            <Field appearance="standard">
              Tracking / dispatch reference
              <TextInput appearance="standard" name="tracking" required minLength={3} maxLength={160} />
            </Field>
            {proof()}
            <AttachmentPanel appearance="standard"
              base={base}
              targetType="inventory_transit"
              targetId={String(d.id)}
              expectedTargetVersion={expectedVersion}
              componentKind="action"
              componentRef="dispatch"
              purposeKey="dispatch_evidence"
              title="Dispatch evidence"
              description="Attach carrier documents, loading photos, seals, or signed dispatch records."
              visibility="Authorized source and receiving inventory teams can access these files."
              onChange={(change) =>
                setAttachmentSets((current) => ({ ...current, dispatch: change }))
              }
            />
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending || !canPrepare}>
              Confirm physical dispatch
            </Button>
          </TransitForm>
          <TransitForm title="Cancel unshipped transport" version={expectedVersion} pending={pending} onDiscard={()=>undefined}
            className="ui-form-grid"
            onCommand={(e) => {
              e.preventDefault();
              const f=new FormData(e.currentTarget);
              return send("cancel", {
                expectedVersion:Number(f.get("sourceVersion")),
                reason: f.get("reason"),
              });
            }}
          >
            <Field appearance="standard">
              Cancellation reason
              <TextArea appearance="standard" name="reason" required minLength={5} maxLength={2000} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={pending || !canSettle}>
              Cancel before dispatch
            </Button>
          </TransitForm>
        </Card>
      )}
      {data.canReceive && canSettle && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Record actual receiving outcome</h2>
          <p>
            Receive only quantities physically counted. Select the exact lot or serial identifiers
            handled at the receiving bay. A loss report reserves the selected disputed balance until
            independent Finance review.
          </p>
          <TransitForm title="Record actual receiving outcome" version={expectedVersion} pending={pending} onDiscard={resetReceiving}
            className="ui-form-grid"
            onCommand={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                quantity = tracked ? String(selectedQuantity) : String(f.get("quantity")),
                allocations = selectedLots.flatMap((l) =>
                  Number(lotQuantities[String(l.id)]) > 0
                    ? [
                        {
                          transitLotId: String(l.id),
                          quantity: String(lotQuantities[String(l.id)]),
                        },
                      ]
                    : [],
                ),
                attachmentKey = `${kind}:${lineId}`;
              return send(kind === "receive" ? "receive" : "loss", {
                expectedVersion:Number(f.get("sourceVersion")),
                attachmentSetIds: setIds(attachmentKey),
                date: f.get("date"),
                evidence: f.get("evidence"),
                ...(kind === "receive"
                  ? { lines: [{ lineId, quantity, allocations }] }
                  : { lineId, quantity, allocations }),
              });
            }}
          >
            <Field appearance="standard">
              Source line
              <Select appearance="standard"
                name="lineId"
                required
                value={lineId}
                onChange={(e) => {
                  setLineId(e.target.value);
                  setLotQuantities({});
                }}
              >
                {receivableLines.map((l) => (
                  <option key={String(l.id)} value={String(l.id)}>
                    {String(l.sku)} · {String(l.name)} ·{" "}
                    {(Number(l.quantity_milli) -
                      Number(l.received_milli) -
                      Number(l.lost_milli) -
                      Number(l.pending_loss_milli)) /
                      1000}{" "}
                    unresolved
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Outcome
              <Select appearance="standard"
                name="kind"
                value={kind}
                onChange={(e) => setKind(e.target.value as "receive" | "loss")}
              >
                <option value="receive">Received into destination stock</option>
                <option value="loss">Report missing / lost in transit</option>
              </Select>
            </Field>
            {tracked ? (
              <Stack as="div" className="ui-stack">
                <strong>
                  Select physical{" "}
                  {String(selectedLine?.tracking_mode) === "serial" ? "serial numbers" : "lots"} ·
                  total {selectedQuantity}
                </strong>
                {selectedLots.map((l) => {
                  const remaining =
                    (Number(l.quantity_milli) -
                      Number(l.received_milli) -
                      Number(l.lost_milli) -
                      Number(l.pending_loss_milli)) /
                    1000;
                  return (
                    <Field appearance="standard" key={String(l.id)}>
                      {l.serial_number
                        ? `Serial ${String(l.serial_number)}`
                        : `Lot ${String(l.lot_code)}`}{" "}
                      · {remaining} available
                      <TextInput appearance="standard"
                        aria-label={`${l.serial_number ? "Serial" : "Lot"} ${String(l.serial_number ?? l.lot_code)} quantity`}
                        type="number"
                        min="0"
                        max={remaining}
                        step={l.serial_number ? "1" : "0.001"}
                        value={lotQuantities[String(l.id)] ?? ""}
                        onChange={(e) =>
                          setLotQuantities({ ...lotQuantities, [String(l.id)]: e.target.value })
                        }
                      />
                    </Field>
                  );
                })}
              </Stack>
            ) : (
              <Field appearance="standard">
                Quantity
                <TextInput appearance="standard" name="quantity" type="number" min="0.001" step="0.001" required />
              </Field>
            )}
            {date()}
            {proof()}
            <AttachmentPanel appearance="standard"
              key={`${kind}:${lineId}:${expectedVersion}`}
              base={base}
              targetType="inventory_transit"
              targetId={String(d.id)}
              expectedTargetVersion={expectedVersion}
              componentKind="event"
              componentRef={`${kind}:${lineId}`}
              purposeKey={kind === "receive" ? "receipt_evidence" : "loss_evidence"}
              title={kind === "receive" ? "Receiving evidence" : "Loss or discrepancy evidence"}
              description={
                kind === "receive"
                  ? "Attach receiving-bay photos, signed receipt records, or package documents."
                  : "Attach damage, seal, carrier, or investigation evidence for the reported discrepancy."
              }
              visibility="Source and receiving inventory teams can access receipt evidence; authorized Company Finance reviewers can access discrepancy evidence."
              onChange={(change) =>
                setAttachmentSets((current) => ({ ...current, [`${kind}:${lineId}`]: change }))
              }
            />
            <Button appearance="standard"
              clientManaged type="submit"
              variant="primary"
              disabled={pending || !canSettle || (tracked && selectedQuantity <= 0)}
            >
              Record receiving outcome
            </Button>
          </TransitForm>
        </Card>
      )}
      {data.canReviewLoss && canSettle &&
        data.events
          .filter((e) => e.kind === "loss_requested" && !e.resolution && Number(e.can_review) > 0)
          .map((p) => (
            <Card as="section" appearance="standard" className="ui-panel" key={String(p.id)}>
              <h2>Independent discrepancy review</h2>
              <p>
                {Number(p.quantity_milli) / 1000} units · reported by {String(p.actor_id)}
              </p>
              <p>{String(p.evidence)}</p>
              <TransitForm title="Independent discrepancy decision" version={expectedVersion} pending={pending} onDiscard={()=>undefined}
                className="ui-form-grid"
                onCommand={(e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget);
                  return send("decide-loss", {
                    expectedVersion:Number(f.get("sourceVersion")),
                    proposalId: p.id,
                    decision: f.get("decision"),
                    date: f.get("date"),
                    evidence: f.get("evidence"),
                    periodId: f.get("periodId") || undefined,
                    expenseAccountId: f.get("expenseAccountId") || undefined,
                  });
                }}
              >
                <Field appearance="standard">
                  Decision
                  <Select appearance="standard" name="decision">
                    <option value="reject">Reject — return to receiving team</option>
                    <option value="approve" disabled={purchase?.lossSettleExisting===false}>Approve loss and post accounting</option>
                  </Select>
                </Field>
                {date()}
                <Field appearance="standard">
                  Open period (required for loss)
                  <Select appearance="standard" name="periodId">
                    <option value="">Select period</option>
                    {data.periods.map((p) => (
                      <option key={String(p.id)} value={String(p.id)}>
                        {String(p.name)}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Loss expense account
                  <Select appearance="standard" name="expenseAccountId">
                    <option value="">Select approved loss account</option>
                    {data.accounts.map((a) => (
                      <option key={String(a.id)} value={String(a.id)}>
                        {String(a.code)} · {String(a.name)}
                      </option>
                    ))}
                  </Select>
                </Field>
                {proof()}
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending || !canSettle}>
                  Record independent decision
                </Button>
              </TransitForm>
            </Card>
          ))}
    </Stack>
  );
}
