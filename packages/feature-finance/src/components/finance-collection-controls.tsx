"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { FinanceEvidenceDocuments } from "@erpx/feature-finance/components/finance-evidence-documents";
import { useContext } from "react";
import { EmbeddedRecord } from "@erpx/ui/components/record-context";
import { recordCanonicalHref } from "@erpx/feature-records/lib/record-drawer";
import { useFinanceCommand } from "@erpx/feature-finance/components/use-finance-command";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import {
  Button,
  Card,
  Field,
  FormLayout,
  InlineNotice,
  Select,
  TextArea,
  TextInput,
  Table,
  TableFrame,
} from "@erpx/ui/components/foundation";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { RecordLink } from "@erpx/feature-records/components/record-link";
import { ListPagination } from "@erpx/ui/components/list-pagination";
type Row = Record<string, unknown>;
export function FinanceCollectionControls({
  base,
  data,
  forecast,
  canWrite,
  canApprove,
  userId,
  query = {},
}: {
  base: string;
  data: {
    asOf: string;
    invoices: Row[];
    cases: Row[];
    pages?: Record<
      "invoices" | "cases",
      { nextCursor: string | null; previousCursor: string | null }
    >;
  };
  forecast: {
    chartId?: string;
    asOf?: string;
    currency: string;
    opening: string;
    basis: string;
    months: {
      month: string;
      movement: string;
      closing: string;
      invoiceIds: string[];
    }[];
  };
  canWrite: boolean;
  canApprove: boolean;
  userId: string;
  query?: Record<string, string | undefined>;
}) {
  const recordOnly = useContext(EmbeddedRecord);
  const {send:run,busy,error,draftScope,canSettle}=useFinanceCommand(base,"finance.receivables");
  return (
    <>
      <Card appearance="standard" as="section" className="ui-panel">
        <h2>Collections</h2>
        <p className="muted">
          Prepare a customer message from the overdue amount, then require
          another person to review it. Delivery verification uses the test
          adapter.
        </p>
        {error && (
          <InlineNotice tone="danger" role="alert">
            {error}
          </InlineNotice>
        )}
        {canWrite && !recordOnly && (
          <FormDisclosure error={error}
            appearance="standard"
            drawerTitle="Prepare a collection case"
          >
            <summary>Prepare customer follow-up</summary>
            <FormLayout data-finance-draft={draftScope}
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget),
                  invoice = data.invoices.find(
                    (r) => r.id === f.get("invoiceId"),
                  );
                void run("collection-cases", {
                  invoiceId: f.get("invoiceId"),
                  expectedVersion: Number(invoice?.version),
                  asOf: data.asOf,
                  message: f.get("message") || undefined,
                });
              }}
            >
              <Field appearance="standard">
                Overdue invoice
                <Select appearance="standard" name="invoiceId" required>
                  <option value="">Select invoice</option>
                  {data.invoices.map((i) => (
                    <option key={String(i.id)} value={String(i.id)}>
                      {String(i.party_name)} · {String(i.invoice_number)} ·{" "}
                      {String(i.currency)}{" "}
                      {minorToDecimal(Number(i.outstanding_minor))}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Message (leave blank to prepare from the invoice)
                <TextArea appearance="standard" name="message" rows={6} />
              </Field>
              <Button
                appearance="standard"
                clientManaged
                type="submit"
                disabled={busy || !canSettle}
              >
                Save case for review
              </Button>
            </FormLayout>
          </FormDisclosure>
        )}
        <TableFrame appearance="standard" label="Overdue invoices">
          <Table>
            <thead>
              <tr>
                <th>Invoice / customer</th>
                <th>Due</th>
                <th>Outstanding</th>
              </tr>
            </thead>
            <tbody>
              {data.invoices.map((i) => (
                <tr key={String(i.id)}>
                  <td>
                    <RecordLink href={canonicalUiHref(`${base}/finance/invoices/${i.id}`)}>
                      {String(i.invoice_number)}
                    </RecordLink>{" "}
                    · {String(i.party_name)}
                  </td>
                  <td>{String(i.due_date)}</td>
                  <td>
                    {String(i.currency)}{" "}
                    {minorToDecimal(Number(i.outstanding_minor))}
                  </td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
        {!recordOnly && data.pages && (
          <div aria-label="Overdue invoice pages">
            <ListPagination
              query={query}
              cursorName="invoiceCursor"
              {...data.pages.invoices}
              count={data.invoices.length}
            />
          </div>
        )}
        <TableFrame appearance="standard" label="Collection cases">
          <Table style={{ minWidth: "42rem" }}>
            <thead>
              <tr>
                <th>Message / source</th>
                <th>Status</th>
                <th>Review / test delivery</th>
              </tr>
            </thead>
            <tbody>
              {data.cases.map((c) => (
                <tr key={String(c.id)}>
                  <td>
                    <RecordLink
                      href={canonicalUiHref(`${base}/finance/invoices/${c.invoice_id}`)}
                    >
                      Invoice
                    </RecordLink>
                    <p>
                      <RecordLink
                        href={recordCanonicalHref(base, {
                          kind: "collection_case",
                          id: String(c.id),
                          args: { asOf: data.asOf },
                        })}
                      >
                        {String(c.message).length > 100 ? `${String(c.message).slice(0,100)}…` : String(c.message)}
                      </RecordLink>
                    </p>
                    <small>
                      {String(c.stage)} · Source version{" "}
                      {String(c.invoice_version)}
                      {c.revision_number != null &&
                        ` · Edition ${c.revision_number}`}
                    </small>
                    {Boolean(c.parent_case_id) && (
                      <p>
                        Revises{" "}
                        <RecordLink
                          href={recordCanonicalHref(base, {
                            kind: "collection_case",
                            id: String(c.parent_case_id),
                            args: { asOf: data.asOf },
                          })}
                        >
                          previous message
                        </RecordLink>{" "}
                        · {String(c.revision_reason)}
                      </p>
                    )}
                  </td>
                  <td>{String(c.status)}</td>
                  <td>
                    <FormDisclosure error={error} appearance="standard" drawerTitle={`Collection evidence · edition ${Number(c.revision_number ?? 1)}`}>
                    <summary>Evidence and review</summary>
                    <p>{String(c.message)}</p>
                    <p className="muted">As of {String(c.as_of_date)} · {String(c.currency)} {minorToDecimal(Number(c.amount_minor))} · {String(c.status)} · Invoice version {String(c.invoice_version)}</p>
                    <FinanceEvidenceDocuments
                      base={base}
                      row={c}
                      targetType="finance_collection_case"
                      canWrite={canWrite}
                    />
                    {canWrite &&
                      Number(c.is_current ?? 1) === 1 &&
                      ["draft", "approved", "cancelled"].includes(
                        String(c.status),
                      ) && (
                        <FormDisclosure error={error}
                          appearance="standard"
                          drawerTitle="Revise unsent collection message"
                        >
                          <summary>Revise message</summary>
                          <FormLayout data-finance-draft={draftScope}
                            className="ui-form-grid"
                            onSubmit={(event) => {
                              event.preventDefault();
                              const form = new FormData(event.currentTarget);
                              void run(`collection-cases/${c.id}/revise`, {
                                expectedVersion: Number(c.version),
                                expectedInvoiceVersion: Number(
                                  c.current_invoice_version ??
                                    c.invoice_version,
                                ),
                                asOf: form.get("asOf"),
                                message: form.get("message") || undefined,
                                reason: form.get("reason"),
                              });
                            }}
                          >
                            <p>
                              Retain the original text and prepare edition{" "}
                              {Number(c.revision_number ?? 1) + 1} for another
                              independent review. Current invoice version{" "}
                              {String(
                                c.current_invoice_version ?? c.invoice_version,
                              )}
                              .
                            </p>
                            <Field appearance="standard">
                              As of
                              <TextInput
                                appearance="standard"
                                type="date"
                                name="asOf"
                                required
                                defaultValue={data.asOf}
                              />
                            </Field>
                            <Field appearance="standard">
                              Revised message
                              <TextArea
                                appearance="standard"
                                name="message"
                                rows={6}
                                defaultValue={String(c.message)}
                              />
                            </Field>
                            <Field appearance="standard">
                              Reason for revision
                              <TextArea
                                appearance="standard"
                                name="reason"
                                required
                                minLength={3}
                              />
                            </Field>
                            <Button
                              appearance="standard"
                              clientManaged
                              type="submit"
                              disabled={busy || !canSettle}
                            >
                              Prepare revised message
                            </Button>
                          </FormLayout>
                        </FormDisclosure>
                      )}
                    {((canApprove &&
                      c.status === "draft" &&
                      c.prepared_by !== userId) ||
                      (canWrite &&
                        ["draft", "approved"].includes(String(c.status)))) && (
                      <FormLayout data-finance-draft={draftScope}
                        onSubmit={(e) => {
                          e.preventDefault();
                          const f = new FormData(e.currentTarget);
                          void run(
                            `collection-cases/${c.id}/${f.get("action")}`,
                            {
                              expectedVersion: Number(c.version),
                              reason: f.get("reason"),
                            },
                          );
                        }}
                      >
                        <Field appearance="standard">
                          Reason
                          <TextInput
                            appearance="standard"
                            name="reason"
                            required
                            minLength={3}
                          />
                        </Field>
                        <Select appearance="standard" name="action">
                          {c.status === "draft" &&
                          canApprove &&
                          c.prepared_by !== userId ? (
                            <option value="approve">Approve message</option>
                          ) : c.status === "approved" ? (
                            <option value="test-deliver">
                              Record test delivery
                            </option>
                          ) : null}
                          <option value="cancel">Cancel case</option>
                        </Select>
                        <Button
                          appearance="standard"
                          clientManaged
                          type="submit"
                          disabled={busy || !canSettle}
                        >
                          Confirm
                        </Button>
                      </FormLayout>
                    )}
                    </FormDisclosure>
                  </td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
        {!recordOnly && data.pages && (
          <div aria-label="Collection case pages">
            <ListPagination
              query={query}
              cursorName="caseCursor"
              {...data.pages.cases}
              count={data.cases.length}
            />
          </div>
        )}
      </Card>
      <Card appearance="standard" as="section" className="ui-panel">
        <h2>Cash from due dates</h2>
        <p>
          {forecast.currency} · Opening bank balance {forecast.opening}
        </p>
        <p className="muted">{forecast.basis}</p>
        <TableFrame appearance="standard" label="Cash due-date forecast">
          <Table>
            <thead>
              <tr>
                <th>Month</th>
                <th>Movement</th>
                <th>Forecast closing cash</th>
                <th>Source obligations</th>
              </tr>
            </thead>
            <tbody>
              {forecast.months.map((m) => (
                <tr key={m.month}>
                  <td>
                    {forecast.chartId && forecast.asOf ? (
                      <RecordLink
                        href={recordCanonicalHref(base, {
                          kind: "cash_forecast_sources",
                          id: m.month,
                          args: {
                            chartId: forecast.chartId,
                            asOf: forecast.asOf,
                          },
                        })}
                      >
                        {m.month}
                      </RecordLink>
                    ) : (
                      m.month
                    )}
                  </td>
                  <td>{m.movement}</td>
                  <td>{m.closing}</td>
                  <td>
                    {m.invoiceIds.map((id) => (
                      <RecordLink
                        key={id}
                        href={canonicalUiHref(`${base}/finance/invoices/${id}`)}
                      >
                        Invoice ·{" "}
                      </RecordLink>
                    ))}
                  </td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
      </Card>
    </>
  );
}
