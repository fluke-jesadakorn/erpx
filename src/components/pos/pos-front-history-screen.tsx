"use client";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button as UiButton, Field, Table, TableFrame, TextInput } from "@/components/ui/foundation";
import type { PosRow } from "@/server/modules/pos/public/pos-core";
import { type FormEvent, type ReactNode } from "react";
import styles from "./pos.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/front-language";
import type {FrontPosDailyContext} from "./pos-front-console";


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
export function FrontPosDailyHistoryScreen({context}: {context: FrontPosDailyContext}) {
  const {tr, refresh, data, money, pick, can, approvalHref} = context;
  return <FrontLocalizedTree>{(
              <section className={styles.panel}>
                <h2>{tr("Search receipts")}</h2>
                <form
                  className={styles.form}
                  onSubmit={(event) => {
                    event.preventDefault();
                    const f = fields(event);
                    const url = new URL(window.location.href);
                    url.searchParams.set("q", text(f, "query"));
                    url.searchParams.set("tab", "history");
                    window.history.pushState(null, "", url);
                    void refresh().catch(() => {});
                  }}
                >
                  <Field>
                    {tr("Receipt number")}
                    <TextInput name="query" maxLength={100} />
                  </Field>
                  <Button type="submit">{tr("Search")}</Button>
                </form>
                <TableFrame label="Records" className={styles.scroll}>
                  <Table className={styles.table}>
                    <thead>
                      <tr>
                        <th>{tr("Receipt")}</th>
                        <th>{tr("Date")}</th>
                        <th>{tr("Total")}</th>
                        <th>{tr("Status")}</th>
                        <th />
                      </tr>
                    </thead>
                    <tbody>
                      {data.sales.map((row) => (
                        <tr key={s(row, "id")}>
                          <td>{s(row, "number")}</td>
                          <td>{s(row, "business_date")}</td>
                          <td>
                            {money(
                              n(row, "total_minor"),
                              s(row, "currency") || null,
                              s(row, "business_date"),
                            )}
                          </td>
                          <td>
                            <Tag value={s(row, "state")} />
                          </td>
                          <td>
                            <Button secondary data-record-link="true" onClick={() => pick(row)}>
                              {tr("Open receipt")}
                            </Button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                </TableFrame>
                <section id="pos-refund-requests" className={styles.spaced}>
                  <h3>{tr("Refund requests")}</h3>
                  {data.refunds.map((row) => (
                    <div className={styles.row} key={s(row, "id")}>
                      <span>
                        <strong>
                          {s(row, "sale_number")} ·{" "}
                          {money(n(row, "amount_minor"), s(row, "currency") || null)}
                        </strong>
                        <p>{s(row, "reason")}</p>
                        <Tag value={s(row, "state")} />
                      </span>
                      {can("pos.approve") && s(row, "state") === "requested" && (
                        <UiActionLink
                          variant="secondary"
                          className={`${styles.button} ${styles.secondary}`}
                          href={approvalHref}
                        >
                          {tr("Review in ERPX")}
                        </UiActionLink>
                      )}
                    </div>
                  ))}
                </section>
              </section>
            )}</FrontLocalizedTree>;
}
