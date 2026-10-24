"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import dynamic from "next/dynamic";
import { RecordLink } from "@erpx/feature-records/components/record-link";
import { recordCanonicalHref } from "@erpx/feature-records/lib/record-drawer";
import { Button as UiButton, Field, Select, Table, TableFrame, TextInput } from "@erpx/ui/components/foundation";
import { FeatureWidget as FeatureWidgetBase } from "@erpx/feature-shared/components/features/feature-page";
import type {PosRow} from '@erpx/server-pos/public/pos-core';
import Link from "@erpx/workplace/components/workspace-link";
import { type ComponentProps, type FormEvent, type ReactNode } from "react";
import styles from "@erpx/feature-pos/components/pos.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@erpx/front/components/front-language";
import { frontText } from "@erpx/feature-shared/lib/front-language";
import type {BackofficePosConsoleContext} from "@erpx/feature-pos/components/standard-pos-backoffice-console";


const PosDayClosePanel = dynamic(() => import("@erpx/feature-pos/components/standard-pos-closing").then(module => module.StandardPosDayClosePanel));


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
export function BackofficePosConsoleReportsScreen({context}: {context: BackofficePosConsoleContext}) {
  const {data, api, base, busy, online, send, erpBase, money, currency, can} = context;
  return <FrontLocalizedTree>{(
                <>
                  <PosDayClosePanel
                    data={data}
                    api={api}
                    base={base}
                    busy={busy || !online}
                    send={send}
                  />
                  <FeatureWidget
                    id="pos-sales-by-date-and-register"
                    title="Sales by date and register"
                    className={styles.panel}
                  >
                    <p className={styles.muted}>
                      Approved returns adjust sales and stock when accepted. Stripe refund payouts
                      remain pending until verified; amounts actually returned appear below by
                      payment method.
                    </p>
                    <TableFrame label="Records" className={styles.scroll}>
                      <Table className={styles.table}>
                        <thead>
                          <tr>
                            <th>Date</th>
                            <th>Register</th>
                            <th>Receipt</th>
                            <th>Sales</th>
                            <th>Discount</th>
                            <th>Tax</th>
                            <th>Approved refunds</th>
                            <th>Net cost</th>
                            <th>Gross profit</th>
                          </tr>
                        </thead>
                        <tbody>
                          {data.reports.map((r, i) => (
                            <tr key={i}>
                              <td><RecordLink href={recordCanonicalHref(erpBase,{kind:'pos_report',id:s(r,'business_date'),args:{registerId:s(r,'register_id'),shiftId:s(r,'shift_id'),currency:s(r,'currency')}})} record={{kind:'pos_report',id:s(r,'business_date'),args:{registerId:s(r,'register_id'),shiftId:s(r,'shift_id'),currency:s(r,'currency')}}}>{s(r, "business_date")}</RecordLink></td>
                              <td>
                                {s(
                                  data.registers.find((reg) => reg.id === r.register_id) ?? {},
                                  "name",
                                )}
                              </td>
                              <td>{n(r, "bills")}</td>
                              <td>
                                {money(
                                  n(r, "total_minor"),
                                  s(r, "currency") || currency,
                                  s(r, "business_date"),
                                )}
                              </td>
                              <td>
                                {money(
                                  n(r, "discount_minor"),
                                  s(r, "currency") || currency,
                                  s(r, "business_date"),
                                )}
                              </td>
                              <td>
                                {money(
                                  n(r, "tax_minor"),
                                  s(r, "currency") || currency,
                                  s(r, "business_date"),
                                )}
                              </td>
                              <td>
                                {money(
                                  n(r, "refunds_minor"),
                                  s(r, "currency") || currency,
                                  s(r, "business_date"),
                                )}
                              </td>
                              <td>
                                {money(
                                  n(r, "cost_minor"),
                                  s(r, "currency") || currency,
                                  s(r, "business_date"),
                                )}
                              </td>
                              <td>
                                {money(
                                  n(r, "gross_profit_minor"),
                                  s(r, "currency") || currency,
                                  s(r, "business_date"),
                                )}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </Table>
                    </TableFrame>
                  </FeatureWidget>
                  <FeatureWidget
                    id="pos-cash-received-by-method-and-cash-session"
                    title="Cash received by method and cash session"
                    className={styles.panel}
                  >
                    {data.paymentMix.map((p, i) => (
                      <div className={styles.row} key={i}>
                        <span>
                          {s(p, "business_date")} · {s(p, "shift_id").slice(0, 8)} ·{" "}
                          {labels[s(p, "method")]}
                        </span>
                        <span>
                          Received{" "}
                          {money(
                            n(p, "received_minor"),
                            s(p, "currency") || currency,
                            s(p, "business_date"),
                          )}{" "}
                          · Refunded{" "}
                          {money(
                            n(p, "refunded_minor"),
                            s(p, "currency") || currency,
                            s(p, "business_date"),
                          )}
                        </span>
                      </div>
                    ))}
                  </FeatureWidget>
                  <FeatureWidget
                    id="pos-stripe-funds-fees-and-disputes"
                    title="Stripe · Funds, fees and disputes"
                    className={styles.panel}
                  >
                    {data.settlementConnection && can("finance.approve") && (
                      <form
                        className={styles.form}
                        onSubmit={(e) => {
                          const f = fields(e);
                          void send("settlement.refresh", {
                            expectedVersion: data.settlementConnection!.version,
                            accountingDate: text(f, "date"),
                            reason: text(f, "reason"),
                          });
                        }}
                      >
                        <Field>
                          Stripe fee accounting date
                          <TextInput name="date" type="date" required />
                        </Field>
                        <Field>
                          Stripe reconciliation reason
                          <TextInput name="reason" minLength={3} required />
                        </Field>
                        <Button type="submit" disabled={busy || !online}>
                          Refresh verified Stripe fees
                        </Button>
                      </form>
                    )}
                    {data.settlements.map((st) => (
                      <div className={styles.row} key={s(st, "id")}>
                        <div>
                          <strong>
                            {s(st, "kind")} · {s(st, "provider_id")}
                          </strong>
                          <Tag value={s(st, "state")} />
                          {st.kind === "payout" && st.state === "unmapped" && (
                            <p>
                              Stripe verified this manual payout. It has no automatic transaction
                              batch. Match an independently posted Finance journal that debits the
                              configured bank account and credits Stripe clearing, then reconcile
                              the bank statement.
                            </p>
                          )}
                        </div>
                        <span>
                          {money(n(st, "amount_minor"), s(st, "currency") || currency)} · Fees{" "}
                          {money(n(st, "fee_minor"), s(st, "currency") || currency)}
                        </span>
                        {st.state === "unmapped" && can("finance.approve") && (
                          <form
                            className={styles.form}
                            onSubmit={(e) => {
                              const f = fields(e);
                              void send("settlement.map_external", {
                                id: st.id,
                                expectedVersion: st.version,
                                journalId: text(f, "journal"),
                                reason: text(f, "reason"),
                              });
                            }}
                          >
                            <Field>
                              Posted external journal
                              <TextInput name="journal" required />
                            </Field>
                            <Field>
                              Reason
                              <TextInput name="reason" required minLength={3} />
                            </Field>
                            <Button type="submit" disabled={busy || !online}>
                              Match external amount
                            </Button>
                          </form>
                        )}
                        {st.kind === "payout" &&
                          st.state === "paid" &&
                          !st.bank_transaction_id &&

                          can("finance.approve") && (
                            <form
                              className={styles.form}
                              onSubmit={(e) => {
                                const f = fields(e),
                                  bank = data.bankTransactions.find(
                                    (b) => b.id === text(f, "bank"),
                                  );
                                if (bank)
                                  void send("settlement.match", {
                                    id: st.id,
                                    expectedVersion: st.version,
                                    bankTransactionId: bank.id,
                                    bankVersion: bank.version,
                                    reason: text(f, "reason"),
                                  });
                              }}
                            >
                              <Field>
                                Bank transaction
                                <Select name="bank" required>
                                  <option value="">Select transaction</option>
                                  {data.bankTransactions
                                    .filter(
                                      (b) =>
                                        b.amount_minor === st.amount_minor &&
                                        b.currency === st.currency,
                                    )
                                    .map((b) => (
                                      <option key={s(b, "id")} value={s(b, "id")}>
                                        {s(b, "transaction_date")} · {s(b, "description")}
                                      </option>
                                    ))}
                                </Select>
                              </Field>
                              <Field>
                                Reconciliation evidence
                                <TextInput name="reason" required minLength={3} />
                              </Field>
                              <Button type="submit" disabled={busy || !online}>
                                Confirm match
                              </Button>
                            </form>
                          )}
                        {st.bank_transaction_id && <span>Bank reconciled</span>}
                        {st.journal_id && (
                          <Link href={canonicalUiHref(`${base}/finance/journals/${st.journal_id}`)}>
                            View ledger
                          </Link>
                        )}
                      </div>
                    ))}
                  </FeatureWidget>
                </>
              )}</FrontLocalizedTree>;
}
