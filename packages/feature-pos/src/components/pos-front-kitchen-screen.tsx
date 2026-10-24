"use client";
import { Button as UiButton, Field, Select, TextInput } from "@erpx/ui/components/foundation";
import type {PosRow} from '@erpx/server-pos/public/pos-core';
import { type FormEvent, type ReactNode } from "react";
import styles from "@erpx/feature-pos/components/pos.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@erpx/front/components/front-language";
import { frontText } from "@erpx/feature-shared/lib/front-language";
import type {FrontPosDailyContext} from "@erpx/feature-pos/components/pos-front-console";


type Row = PosRow;


const s = (r: Row, k: string) => String(r[k] ?? "");


const n = (r: Row, k: string) => Number(r[k] ?? 0);


const kitchenCommands: Record<string, string> = {
  draft: "kitchen.send", queued: "kitchen.start",
  preparing: "kitchen.ready", ready: "kitchen.deliver",
};


// Sending an order and serving it belong to the cashier; preparation belongs to the kitchen.
const kitchenPermission = (state: string) => state === "draft" || state === "ready"
  ? "pos.sell" : state === "queued" || state === "preparing" ? "pos.kitchen" : null;


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


function KitchenInstructions({ item }: { item: Row }) {
  const modifiers = (JSON.parse(s(item, "modifiers_json") || "[]") as Array<{ name: string }>)
    .map((m) => m.name)
    .join(" · ");
  return (
    <>
      {s(item, "note") && <p style={{ whiteSpace: "pre-wrap" }}>{s(item, "note")}</p>}
      {modifiers && <p>{modifiers}</p>}
    </>
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
export function FrontPosDailyKitchenScreen({context}: {context: FrontPosDailyContext}) {
  const {tr, kitchenStation, setKitchenStation, data, can, busy, online, send} = context;
  return <FrontLocalizedTree>{(
              <section className={styles.panel}>
                <h2>{tr("Kitchen queue")}</h2>
                <Field>
                  {tr("Kitchen station")}
                  <Select
                    value={kitchenStation}
                    onChange={(event) => setKitchenStation(event.target.value)}
                  >
                    <option value="">{tr("All stations")}</option>
                    {[...new Set(data.kitchen.map((row) => s(row, "station")))]
                      .filter(Boolean)
                      .map((item) => (
                        <option key={item}>{item}</option>
                      ))}
                  </Select>
                </Field>
                {data.kitchen
                  .filter((row) => !kitchenStation || s(row, "station") == kitchenStation)
                  .map((item) => (
                    <article className={styles.record} key={s(item, "id")}>
                      <div className={styles.line}>
                        <h3>
                          {s(item, "product_name")} × {n(item, "quantity_milli") / 1000}
                        </h3>
                        <Tag value={s(item, "state")} kitchen />
                      </div>
                      <p>
                        {s(item, "station")} · {s(item, "sale_numbers")}
                      </p>
                      <KitchenInstructions item={item} />
                      <div className={styles.actions}>
                        {kitchenPermission(s(item, "state")) && can(kitchenPermission(s(item, "state"))!) && (
                          <Button
                            disabled={busy || !online}
                            onClick={() =>
                              void send(
                                kitchenCommands[s(item, "state")],
                                { id: item.id, expectedVersion: item.version },
                              )
                            }
                          >
                            {tr(
                              (
                                {
                                  draft: "Send to kitchen",
                                  queued: "Start · consume ingredients",
                                  preparing: "Ready to serve",
                                  ready: "Delivered",
                                } as Record<string, string>
                              )[s(item, "state")] ?? "Update",
                            )}
                          </Button>
                        )}
                      </div>
                      {can("pos.approve") && (
                        <details className={styles.spaced}>
                          <summary>{tr("Cancel / record waste")}</summary>
                          <form
                            className={styles.form}
                            onSubmit={(event) => {
                              const f = fields(event);
                              void send("kitchen.cancel", {
                                id: item.id,
                                expectedVersion: item.version,
                                reason: text(f, "reason"),
                              });
                            }}
                          >
                            <Field>
                              {tr("Reason")}
                              <TextInput name="reason" minLength={3} required />
                            </Field>
                            <Button type="submit" disabled={busy || !online}>
                              {tr("Confirm cancellation")}
                            </Button>
                          </form>
                        </details>
                      )}
                    </article>
                  ))}
                {!data.kitchen.length && (
                  <p className={styles.empty}>{tr("No kitchen items pending")}</p>
                )}
              </section>
            )}</FrontLocalizedTree>;
}
