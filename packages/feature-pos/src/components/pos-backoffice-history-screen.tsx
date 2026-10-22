"use client";
import dynamic from "next/dynamic";
import { Button as UiButton, Field, Table, TableFrame, TextInput } from "@erpx/ui/components/foundation";
import { FeatureWidget as FeatureWidgetBase } from "@erpx/feature-shared/components/features/feature-page";
import { ListPagination } from "@erpx/ui/components/list-pagination";
import type { PosRow } from "@erpx/server-pos/public/pos-core";
import { type ComponentProps, type FormEvent, type ReactNode } from "react";
import styles from "@erpx/feature-pos/components/pos.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@erpx/front/components/front-language";
import { frontText } from "@erpx/feature-shared/lib/front-language";
import type {BackofficePosConsoleContext} from "@erpx/feature-pos/components/standard-pos-backoffice-console";


const AttachmentPanel = dynamic(() => import("@erpx/feature-files/components/attachment-panel").then(module => module.AttachmentPanel));


type Row = PosRow;


const s = (r: Row, k: string) => String(r[k] ?? "");


const n = (r: Row, k: string) => Number(r[k] ?? 0);


const fields = (e: FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  return new FormData(e.currentTarget);
};


const text = (f: FormData, k: string) => String(f.get(k) ?? "");


const labels: Record<string, string> = {
  open: "Open",
  held: "Held",
  payment_pending: "Payment pending",
  completed: "Succeeded",
  cancelled: "Cancelled",
  merged: "Merged",
  draft: "Not sent",
  queued: "Kitchen pending",
  preparing: "In progress",
  ready: "Ready",
  delivered: "Delivered",
  requested: "Pending",
  pending: "Awaiting result",
  succeeded: "Succeeded",
  failed: "Failed",
  exception: "Needs review",
  closed: "Cash session closed",
  reviewed: "Reviewed",
  approved: "Approved",
  rejected: "Rejected",
  processing: "Refunding",
  onboarding: "Onboarding",
  unconfigured: "Not configured",
  active: "Active",
  inactive: "Inactive",
  cash: "Cash",
  transfer: "Bank transfer",
  edc: "External EDC",
  stripe_card: "Stripe card",
  stripe_promptpay: "PromptPay / Stripe",
};


const kitchenLabels: Record<string, [string, string]> = {
  draft: ["ยังไม่ส่งครัว", "Not sent"],
  queued: ["รอครัวรับงาน", "Kitchen pending"],
  preparing: ["กำลังทำ", "In progress"],
  ready: ["พร้อมเสิร์ฟ", "Ready to serve"],
  delivered: ["ส่งมอบแล้ว", "Delivered"],
  cancelled: ["ยกเลิกแล้ว", "Cancelled"],
};


function Tag({ value, kitchen = false }: { value: string; kitchen?: boolean }) {
  const language = useFrontLanguage();
  return (
    <span className={styles.tag}>
      {kitchen && kitchenLabels[value]
        ? kitchenLabels[value][language === "th" ? 0 : 1]
        : frontText(labels[value] ?? value, language)}
    </span>
  );
}


function FeatureWidget(props: ComponentProps<typeof FeatureWidgetBase>) {
  const language = useFrontLanguage();
  return <FeatureWidgetBase {...props} title={frontText(props.title, language)} />;
}


function Button({
  children,
  onClick,
  disabled,
  secondary = false,
  type = "button",
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  secondary?: boolean;
  type?: "button" | "submit";
}) {
  const language = useFrontLanguage();
  return (
    <UiButton
      variant="secondary"
      type={type}
      clientManaged={type === "submit"}
      onClick={onClick}
      disabled={disabled}
      className={`${styles.button} ${secondary ? styles.secondary : ""}`}
    >
      {typeof children === "string" ? frontText(children, language) : children}
    </UiButton>
  );
}
export function BackofficePosConsoleHistoryScreen({context}: {context: BackofficePosConsoleContext}) {
  const {data, tab, historyQuery, cursor, money, currency, pickSale, can, send, base, busy, online} = context;
  return <FrontLocalizedTree>{(
                <>
                  {data.salesPage && (
                    <ListPagination
                      {...data.salesPage}
                      query={{ tab, q: historyQuery, cursor }}
                      count={data.sales.length}
                    />
                  )}
                  <FeatureWidget
                    id="pos-search-receipts"
                    title="Search receipts"
                    className={styles.panel}
                  >
                    <form
                      className={styles.form}
                      onSubmit={(e) => {
                        const f = fields(e);
                        const url = new URL(window.location.href);
                        url.searchParams.delete("cursor");
                        url.searchParams.set("q", text(f, "query"));
                        window.history.replaceState(null, "", url);
                      }}
                    >
                      <Field>
                        Receipt number
                        <TextInput name="query" maxLength={100} />
                      </Field>
                      <Button type="submit">Search</Button>
                    </form>
                    <TableFrame label="Records" className={styles.scroll}>
                      <Table className={styles.table}>
                        <thead>
                          <tr>
                            <th>Receipt</th>
                            <th>Register</th>
                            <th>Date</th>
                            <th>Total</th>
                            <th>Status</th>
                            <th />
                          </tr>
                        </thead>
                        <tbody>
                          {data.sales.map((row) => (
                            <tr key={s(row, "id")}>
                              <td>{s(row, "number")}</td>
                              <td>{s(row, "register_name")}</td>
                              <td>{s(row, "business_date")}</td>
                              <td>
                                {money(
                                  n(row, "total_minor"),
                                  s(row, "currency") || currency,
                                  s(row, "business_date"),
                                )}
                              </td>
                              <td>
                                <Tag value={s(row, "state")} />
                              </td>
                              <td>
                                <Button secondary data-record-link="true" onClick={() => pickSale(row)}>
                                  Open receipt
                                </Button>
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </Table>
                    </TableFrame>
                  </FeatureWidget>
                  <FeatureWidget
                    id="pos-refund-requests"
                    title="Refund requests"
                    className={styles.panel}
                  >
                    {data.refunds.map((r) => (
                      <div className={styles.row} key={s(r, "id")}>
                        <div>
                          <strong>
                            {s(r, "sale_number")} ·{" "}
                            {money(n(r, "amount_minor"), s(r, "currency") || currency)}
                          </strong>
                          <p>{s(r, "reason")}</p>
                          <Tag value={s(r, "state")} />
                        </div>
                        {can("pos.approve") && r.state === "requested" && (
                          <form
                            className={styles.form}
                            onSubmit={(e) => {
                              const f = fields(e),
                                setId = text(f, "attachmentSetId");
                              void send("refund.approve", {
                                id: r.id,
                                expectedVersion: r.version,
                                reason: text(f, "reason"),
                                attachmentSetIds: setId ? [setId] : [],
                              });
                            }}
                          >
                            <Field>
                              Refund evidence / reason
                              <TextInput name="reason" minLength={3} required />
                            </Field>
                            <AttachmentPanel
                              base={base}
                              targetType="pos_refund"
                              targetId={s(r, "id")}
                              expectedTargetVersion={n(r, "version")}
                              componentKind="action"
                              componentRef="refund-decision"
                              purposeKey="refund_evidence"
                              title="Refund evidence"
                              description="Attach the reviewed refund proof, customer correspondence, or returned-item photo."
                              visibility="Visible to authorized POS reviewers in this Branch."
                              compact
                              fieldName="attachmentSetId"
                            />
                            <Button type="submit" disabled={busy || !online}>
                              Approve and confirm refund
                            </Button>
                            <Button
                              secondary
                              disabled={busy || !online}
                              onClick={() =>
                                void send("refund.reject", {
                                  id: r.id,
                                  expectedVersion: r.version,
                                  reason: "Reject refund request",
                                  attachmentSetIds: [],
                                })
                              }
                            >
                              Reject
                            </Button>
                          </form>
                        )}
                      </div>
                    ))}
                  </FeatureWidget>
                </>
              )}</FrontLocalizedTree>;
}
