"use client";
import dynamic from "next/dynamic";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { Button as UiButton, Field, Select, TextInput } from "@erpx/ui/components/foundation";
import type { PosRow } from "@erpx/server-pos/public/pos-core";
import { type FormEvent, type ReactNode } from "react";
import styles from "@erpx/feature-pos/components/pos.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@erpx/front/components/front-language";
import { frontText } from "@erpx/feature-shared/lib/front-language";
import type {StandardFrontPosDailyContext as FrontPosDailyContext} from "@erpx/feature-pos/components/standard-pos-front-console";


const CashDenominationCount = dynamic(() => import("@erpx/feature-pos/components/standard-pos-closing").then(module => module.StandardCashDenominationCount));


type Row = PosRow;


const s = (r: Row, k: string) => String(r[k] ?? "");


const n = (r: Row, k: string) => Number(r[k] ?? 0);


const fields = (e: FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  return new FormData(e.currentTarget);
};


const text = (f: FormData, k: string) => String(f.get(k) ?? "");


const amount = (f: FormData, k: string) => Math.round(Number(f.get(k) ?? 0) * 100);


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
export function FrontPosDailyShiftsScreen({context}: {context: FrontPosDailyContext}) {
  const {tr, can, openingDraft, send, activeRegisterId, setOpeningDraft, data, busy, online, money, language, approvalHref} = context;
  return <FrontLocalizedTree>{(
              <section className={styles.panel}>
                <h2>{tr("Cash shifts")}</h2>
                {can("pos.sell") && (
                  <form
                    className={styles.form}
                    onSubmit={(event) => {
                      const f = fields(event);
                      const submittedDraft = openingDraft;
                      void send("shift.open", {
                        registerId: text(f, "register") || activeRegisterId,
                        openingMinor: amount(f, "opening"),
                      },false,undefined,{form:event.currentTarget,acknowledged:()=>{
                        setOpeningDraft((current) => current === submittedDraft
                          ? {...current, opening: "0"} : current);
                      }});
                    }}
                  >
                    <Field>
                      {tr("Register")}
                      <Select name="register" value={openingDraft.registerId}
                        onChange={(event) => setOpeningDraft((current) => ({
                          ...current, registerId: event.target.value, opening: "0",
                        }))}>
                        {data.registers
                          .filter((row) => Boolean(row.active))
                          .map((row) => (
                            <option key={s(row, "id")} value={s(row, "id")}>
                              {s(row, "name")}
                            </option>
                          ))}
                      </Select>
                    </Field>
                    <Field>
                      {tr("Opening float")}
                      <TextInput
                        name="opening"
                        type="number"
                        min="0"
                        step="0.01"
                        value={openingDraft.opening}
                        onChange={(event) => setOpeningDraft((current) => ({
                          ...current, opening: event.target.value,
                        }))}
                        required
                      />
                    </Field>
                    <Button type="submit" disabled={busy || !online || data.purchases && !data.purchases["pos.shifts"].newWork}>
                      {tr("Open cash shift")}
                    </Button>
                  </form>
                )}
                {data.shifts.map((item) => (
                  <article className={styles.record} key={s(item, "id")}>
                    <div className={styles.line}>
                      <h3>{s(item, "register_name")}</h3>
                      <Tag value={s(item, "state")} />
                    </div>
                    <p>
                      {tr("Business date")} {s(item, "business_date")}
                    </p>
                    <p>
                      {tr("Opening cash")} {money(n(item, "opening_minor"))}
                    </p>
                    {s(item, "state") !== "open" && (
                      <dl className={styles.fields}>
                        <div>
                          <dt>{language === "th" ? "ยอดคาดหวัง" : "Expected cash"}</dt>
                          <dd>{money(n(item, "expected_minor"))}</dd>
                        </div>
                        <div>
                          <dt>{tr("Counted cash")}</dt>
                          <dd>{money(n(item, "counted_minor"))}</dd>
                        </div>
                        <div>
                          <dt>{language === "th" ? "เงินขาด / เกิน" : "Short / over"}</dt>
                          <dd>{money(n(item, "variance_minor"))}</dd>
                        </div>
                      </dl>
                    )}
                    {s(item, "state") === "open" && can("pos.sell") && (
                      <>
                        <form
                          className={styles.form}
                          onSubmit={(event) => {
                            const f = fields(event);
                            void send("shift.close", {
                              id: item.id,
                              expectedVersion: item.version,
                              countedMinor: amount(f, "counted"),
                              ...(f.get("denominations")
                                ? { denominations: JSON.parse(text(f, "denominations")) }
                                : {}),
                            });
                          }}
                        >
                          <CashDenominationCount
                            currency={data.currency ?? "THB"}
                            expectedMinor={n(item, "cash_expected_minor")}
                          />
                          <Button type="submit" disabled={busy || !online}>
                            {tr("Close cash shift")}
                          </Button>
                        </form>
                        <details>
                          <summary>{tr("Cash drawer movement")}</summary>
                          <form
                            className={styles.form}
                            onSubmit={(event) => {
                              const f = fields(event);
                              void send("cash.create", {
                                id: item.id,
                                expectedVersion: item.version,
                                amountMinor: amount(f, "amount"),
                                reason: text(f, "reason"),
                              });
                            }}
                          >
                            <Field>
                              {tr("Amount (+in / −out)")}
                              <TextInput name="amount" type="number" step="0.01" required />
                            </Field>
                            <Field>
                              {tr("Reason")}
                              <TextInput name="reason" required minLength={3} />
                            </Field>
                            <Button type="submit" disabled={busy || !online}>
                              {tr("Send request")}
                            </Button>
                          </form>
                        </details>
                      </>
                    )}
                    {s(item, "state") === "closed" && can("pos.approve") && (
                      <UiActionLink
                        variant="primary"
                        className={styles.button}
                        href={approvalHref.replace("tab=history", "tab=shifts")}
                      >
                        {tr("Review in ERPX")}
                      </UiActionLink>
                    )}
                    {data.cashMovements
                      .filter((movement) => movement.shift_id === item.id)
                      .map((movement) => (
                        <div className={styles.row} key={s(movement, "id")}>
                          <span>
                            {money(n(movement, "amount_minor"))} · {s(movement, "reason")}
                          </span>
                          {movement.approved_by ? (
                            <Tag value="approved" />
                          ) : (
                            can("pos.approve") &&
                            s(item, "state") === "open" && (
                              <UiActionLink
                                variant="secondary"
                                className={`${styles.button} ${styles.secondary}`}
                                href={approvalHref.replace("tab=history", "tab=shifts")}
                              >
                                {tr("Review in ERPX")}
                              </UiActionLink>
                            )
                          )}
                        </div>
                      ))}
                  </article>
                ))}
              </section>
            )}</FrontLocalizedTree>;
}
