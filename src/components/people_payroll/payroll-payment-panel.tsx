"use client";
import {PayrollCommandRecovery} from "./payroll-command-recovery";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { RecordLink as Link } from "@/components/records/record-link";
import { MoneyAmount } from "@/components/finance/money/currency-provider";
import { usePayrollCommand } from "./use-payroll-command";
import { PayrollForm } from "./payroll-draft-boundary";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import {useState} from "react";
import { Card } from "@/components/ui/foundation";
type Row = Record<string, unknown>;
export function PayrollPaymentPanel({
  base,
  run,
  batch,
  items,
  canPrepare,
  canApprove,
  canExport,
  canConfirm,
}: {
  base: string;
  run: Row;
  batch: Row | null;
  items: Row[];
  userId: string;
  canPrepare: boolean;
  canApprove: boolean;
  canExport: boolean;
  canConfirm: boolean;
}) {
  const status = (value: unknown) => {
    const labels: Record<string, string> = {
      draft: "Draft",
      approved: "Approved",
      exported: "Exported",
      partially_settled: "Partially settled",
      settled: "Settled",
      outstanding: "Outstanding",
      failed: "Rejected",
      confirmed: "Confirmed",
    };
    return labels[String(value)] ?? String(value);
  };
  const {send, busy, error, notice, canSettle, canRead, recovery} = usePayrollCommand(base, `run:${run.id}:payments`, JSON.stringify([run, batch, items, canPrepare, canApprove, canExport, canConfirm]));
  const [drawerRevisions, setDrawerRevisions] = useState<Record<string, number>>({});
  const discard = (name: string) => setDrawerRevisions(current => ({...current, [name]: (current[name] ?? 0) + 1}));
  canPrepare &&= canSettle; canApprove &&= canSettle; canExport &&= canSettle; canConfirm &&= canSettle;
  const outstanding = items.filter((item) => ["outstanding", "failed"].includes(String(item.status)));
  return (
    <Card as="section" appearance="standard" className="ui-panel" aria-label="Payroll payment batch">
      <h2>{"Payment batch"}</h2>
      <p className="muted">{"Exported instructions do not transfer funds from this page."}</p>
      <PayrollCommandRecovery {...recovery} appearance="standard"/>{error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{notice && <p role="status">{notice}</p>}
      {!batch && canPrepare && String(run.status) === "posted" && (
        <FormDisclosure appearance="standard" drawerTitle="Prepare payroll payment batch" onDiscard={() => discard("prepare")}><summary>Prepare payment batch</summary><PayrollForm key={drawerRevisions.prepare ?? 0}
          className="ui-form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            void send("payments/batches", {
              sourceType: "payroll",
              sourceId: String(run.id),
              formatCode: form.get("formatCode"),
              bankAccountId: form.get("bankAccountId"),
            });
          }}
        >
          <p>{"Prepare from posted net payable."}</p>
          <Field appearance="standard">
            {"Export format"}
            <Select appearance="standard" name="formatCode" defaultValue="generic-csv">
              <option value="generic-csv">Generic CSV sandbox</option>
              <option value="iso20022">ISO 20022 sandbox</option>
            </Select>
          </Field>
          <Field appearance="standard">
            {"Finance bank account ID"}
            <TextInput appearance="standard" name="bankAccountId" required />
          </Field>
          <p className="muted">
            {"Review bank accounts in"}{" "}
            <Link href={canonicalUiHref(`${base}/finance/bank`)}>Finance · Cash & banking</Link>
          </p>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            {"Prepare batch"}
          </Button>
        </PayrollForm></FormDisclosure>
      )}
      {!batch && !canPrepare && <p className="muted">{"No payment batch yet."}</p>}
      {batch && (
        <>
          <p>
            <strong>
              <MoneyAmount
                units={String(batch.total_minor)}
                currency={String(batch.currency)}
                legacyScale={2}
              />
            </strong>{" "}
            · {items.length} {"payees"} · {status(batch.status)}
          </p>
          <div className="ui-form-actions">
            {canApprove && batch.status === "draft" && (
              <Button appearance="standard"
                type="submit"
                variant="primary"
                disabled={busy}
                onClick={() =>
                  void send(`payments/batches/${batch.id}/approve`, {
                    expectedVersion: Number(batch.version),
                  })
                }
              >
                {"Approve batch"}
              </Button>
            )}
            {canExport && batch.status === "approved" && (
              <Button appearance="standard"
                type="submit"
                variant="primary"
                disabled={busy}
                onClick={() => void send(`payments/batches/${batch.id}/export`, {})}
              >
                {"Export instruction"}
              </Button>
            )}
            {canRead && ["exported", "partially_settled", "settled"].includes(String(batch.status)) && (
              <UiActionLink appearance="standard"
                variant="secondary"
                href={`/api/v1${base}/people/payments/batches/${batch.id}/file`}
              >
                {"Download file"}
              </UiActionLink>
            )}
            {canPrepare &&
              batch.status === "partially_settled" &&
              items.some((item) => item.status === "failed") && (
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"
                  disabled={busy}
                  onClick={() =>
                    void send(`payments/batches/${batch.id}/retry`, {
                      expectedVersion: Number(batch.version),
                    })
                  }
                >
                  {"Prepare retry for rejected rows"}
                </Button>
              )}
          </div>
          {batch.status === "partially_settled" && (
            <p className="muted">
              {"A retry requires another approval. The new file includes rejected rows only."}
            </p>
          )}
          {canConfirm &&
            ["exported", "partially_settled"].includes(String(batch.status)) &&
            outstanding.length > 0 && (
              <FormDisclosure appearance="standard" drawerTitle="Reconcile payroll bank results" onDiscard={() => discard("reconcile")}><summary>Reconcile bank results</summary><PayrollForm key={drawerRevisions.reconcile ?? 0}
                className="ui-form-grid"
                onSubmit={(event) => {
                  event.preventDefault();
                  const form = new FormData(event.currentTarget);
                  const rows = outstanding.map((item) => {
                    const reference = String(item.reference);
                    const status = String(form.get(`status:${reference}`));
                    const bankVersion = Number(form.get(`bankVersion:${reference}`));
                    return {
                      reference,
                      status,
                      confirmationRef: String(form.get(`ref:${reference}`) ?? "") || undefined,
                      ...(status === "confirmed"
                        ? {
                            bankTransactionId:
                              String(form.get(`bankId:${reference}`) ?? "") || undefined,
                            bankVersion:
                              Number.isInteger(bankVersion) && bankVersion > 0
                                ? bankVersion
                                : undefined,
                          }
                        : {}),
                    };
                  });
                  void send(`payments/batches/${batch.id}/confirm`, { rows });
                }}
              >
                <h3>{"Reconcile bank results"}</h3>
                <p className="muted">
                  {"Confirm only accepted payments with transaction ID and version from"}{" "}
                  <Link href={canonicalUiHref(`${base}/finance/bank`)}>{"Bank transactions"}</Link>.{" "}
                  {"Rejected rows remain payable."}
                </p>
                {outstanding.map((item) => (
                  <div className="ui-record-item" key={String(item.reference)}>
                    <div className="row-main">
                      <strong>{String(item.payee_name)}</strong>
                      <span>
                        {String(item.reference)} ·{" "}
                        <MoneyAmount
                          units={String(item.amount_minor)}
                          currency={String(item.currency)}
                          legacyScale={2}
                        />{" "}
                        · {status(item.status)}
                      </span>
                    </div>
                    <Field appearance="standard">
                      {"Result"}
                      <Select appearance="standard" name={`status:${String(item.reference)}`} defaultValue="confirmed">
                        <option value="confirmed">{"Confirmed"}</option>
                        <option value="failed">{"Failed"}</option>
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      {"Bank reference"}
                      <TextInput appearance="standard" name={`ref:${String(item.reference)}`} required />
                    </Field>
                    <Field appearance="standard">
                      {"Bank transaction ID"}
                      <TextInput appearance="standard" name={`bankId:${String(item.reference)}`} />
                    </Field>
                    <Field appearance="standard">
                      {"Bank transaction version"}
                      <TextInput appearance="standard"
                        name={`bankVersion:${String(item.reference)}`}
                        type="number"
                        min="1"
                        step="1"
                      />
                    </Field>
                  </div>
                ))}
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  {"Confirm results"}
                </Button>
              </PayrollForm></FormDisclosure>
            )}
          {batch.status === "partially_settled" && !outstanding.length && (
            <InlineNotice tone="info" role="status">{"Refresh to review remaining items."}</InlineNotice>
          )}
        </>
      )}
    </Card>
  );
}
