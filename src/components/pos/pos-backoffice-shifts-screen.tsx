"use client";
import dynamic from "next/dynamic";
import { Button as UiButton, Field, Select, TextInput } from "@/components/ui/foundation";
import { FeatureWidget as FeatureWidgetBase } from "@/components/shared/features/feature-page";
import type { PosRow } from "@/server/modules/pos/public/pos-core";
import { type ComponentProps, type FormEvent, type ReactNode } from "react";
import styles from "./pos.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {BackofficePosConsoleContext} from "./pos-backoffice-console";


const CashDenominationCount = dynamic(() => import("./pos-closing").then(module => module.CashDenominationCount));


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
export function BackofficePosConsoleShiftsScreen({context}: {context: BackofficePosConsoleContext}) {
  const {send, registerId, data, tr, currency, busy, can, money, online} = context;
  return <FrontLocalizedTree>{(
                <>
                  <FeatureWidget
                    id="pos-open-cash-session"
                    title="Open cash session"
                    className={styles.panel}
                  >
                    <form
                      className={styles.form}
                      onSubmit={(e) => {
                        const f = fields(e);
                        void send("shift.open", {
                          registerId: text(f, "register"),
                          openingMinor: amount(f, "opening"),
                        });
                      }}
                    >
                      <Field>
                        Register
                        <Select name="register" defaultValue={registerId}>
                          {data.registers
                            .filter((r) => r.active)
                            .map((r) => (
                              <option value={s(r, "id")} key={s(r, "id")}>
                                {s(r, "name")}
                              </option>
                            ))}
                        </Select>
                      </Field>
                      <Field>
                        <span>
                          {tr("Opening float")}{" "}
                          {currency && <small className={styles.currencyUnit}>({currency})</small>}
                        </span>
                        <TextInput
                          type="number"
                          name="opening"
                          min="0"
                          step="0.01"
                          defaultValue="0"
                          required
                        />
                      </Field>
                      <Button type="submit" disabled={busy || !can("pos.sell")}>
                        Open cash session
                      </Button>
                    </form>
                  </FeatureWidget>
                  <div className={styles.grid}>
                    {data.shifts.map((sh) => (
                      <section className={styles.panel} key={s(sh, "id")}>
                        <div className={styles.header}>
                          <h2>{s(sh, "register_name")}</h2>
                          <Tag value={s(sh, "state")} />
                        </div>
                        <p>Opening cash {money(n(sh, "opening_minor"))}</p>
                        {sh.state !== "open" && (
                          <p>
                            Expected {money(n(sh, "expected_minor"))} · Counted{" "}
                            {money(n(sh, "counted_minor"))}
                            <br />
                            Short / over {money(n(sh, "variance_minor"))}
                          </p>
                        )}
                        {sh.state === "open" && can("pos.sell") && (
                          <>
                            <form
                              className={styles.form}
                              onSubmit={(e) => {
                                const f = fields(e);
                                void send("shift.close", {
                                  id: sh.id,
                                  expectedVersion: sh.version,
                                  countedMinor: amount(f, "counted"),
                                  ...(f.get("denominations")
                                    ? { denominations: JSON.parse(text(f, "denominations")) }
                                    : {}),
                                });
                              }}
                            >
                              <CashDenominationCount
                                currency={currency ?? "THB"}
                                expectedMinor={n(sh, "cash_expected_minor")}
                              />
                              <Button type="submit" disabled={busy || !online}>
                                Close cash session
                              </Button>
                            </form>
                            <details className={styles.spaced}>
                              <summary>Cash drawer movement</summary>
                              <form
                                className={styles.form}
                                onSubmit={(e) => {
                                  const f = fields(e);
                                  void send("cash.create", {
                                    id: sh.id,
                                    expectedVersion: sh.version,
                                    amountMinor: amount(f, "amount"),
                                    reason: text(f, "reason"),
                                  });
                                }}
                              >
                                <Field>
                                  <span>
                                    {tr("Amount (+in / −out)")}{" "}
                                    {currency && (
                                      <small className={styles.currencyUnit}>({currency})</small>
                                    )}
                                  </span>
                                  <TextInput name="amount" type="number" step="0.01" required />
                                </Field>
                                <Field>
                                  Reason
                                  <TextInput name="reason" required minLength={3} />
                                </Field>
                                <Button type="submit" disabled={busy || !online}>
                                  Request approval
                                </Button>
                              </form>
                            </details>
                          </>
                        )}
                        {data.cashMovements
                          .filter((m) => m.shift_id === sh.id)
                          .map((m) => (
                            <div key={s(m, "id")} className={styles.row}>
                              <span>
                                {money(n(m, "amount_minor"))} · {s(m, "reason")}
                              </span>
                              {m.approved_by ? (
                                <Tag value="approved" />
                              ) : (
                                can("pos.approve") && (
                                  <Button
                                    secondary
                                    disabled={busy || !online}
                                    onClick={() =>
                                      void send("cash.approve", {
                                        id: sh.id,
                                        expectedVersion: sh.version,
                                        movementId: m.id,
                                      })
                                    }
                                  >
                                    Approved
                                  </Button>
                                )
                              )}
                            </div>
                          ))}
                      </section>
                    ))}
                  </div>
                </>
              )}</FrontLocalizedTree>;
}
