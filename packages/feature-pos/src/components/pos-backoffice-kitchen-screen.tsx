"use client";
import { Button as UiButton, Field, Select, TextInput } from "@erpx/ui/components/foundation";
import { FeatureWidget as FeatureWidgetBase } from "@erpx/feature-shared/components/features/feature-page";
import type {PosRow} from '@erpx/server-pos/public/pos-core';
import { type ComponentProps, type FormEvent, type ReactNode } from "react";
import styles from "@erpx/feature-pos/components/pos.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@erpx/front/components/front-language";
import { frontText } from "@erpx/feature-shared/lib/front-language";
import type {BackofficePosConsoleContext} from "@erpx/feature-pos/components/standard-pos-backoffice-console";


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
export function BackofficePosConsoleKitchenScreen({context}: {context: BackofficePosConsoleContext}) {
  const {kitchenStation, setKitchenStation, data, can, busy, online, send} = context;
  return <FrontLocalizedTree>{(
                <FeatureWidget id="pos-kitchen-queue" title="Kitchen queue">
                  <Field>
                    Kitchen station{" "}
                    <Select
                      value={kitchenStation}
                      onChange={(e) => setKitchenStation(e.target.value)}
                    >
                      <option value="">All stations</option>
                      {[...new Set(data.kitchen.map((f) => s(f, "station")))].map((name) => (
                        <option key={name}>{name}</option>
                      ))}
                    </Select>
                  </Field>
                  <div className={styles.grid}>
                    {data.kitchen
                      .filter((f) => !kitchenStation || f.station === kitchenStation)
                      .map((f) => (
                        <section className={styles.panel} key={s(f, "id")}>
                          <div className={styles.header}>
                            <h2>
                              {s(f, "product_name")} × {n(f, "quantity_milli") / 1000}
                            </h2>
                            <Tag value={s(f, "state")} kitchen />
                          </div>
                          <p>
                            {s(f, "station")} · {s(f, "sale_numbers")}
                          </p>
                          <KitchenInstructions item={f} />
                          <div className={styles.actions}>
                            {kitchenPermission(s(f, "state")) && can(kitchenPermission(s(f, "state"))!) && (
                              <Button
                                disabled={busy || !online}
                                onClick={() =>
                                  void send(
                                    kitchenCommands[s(f, "state")],
                                    { id: f.id, expectedVersion: f.version },
                                  )
                                }
                              >
                                {
                                  (
                                    {
                                      draft: "Send to kitchen",
                                      queued: "Start · consume ingredients",
                                      preparing: "Ready to serve",
                                      ready: "Delivered",
                                    } as Record<string, string>
                                  )[s(f, "state")]
                                }
                              </Button>
                            )}
                          </div>
                          {can("pos.approve") && (
                            <details className={styles.spaced}>
                              <summary>Cancel / record waste</summary>
                              <form
                                className={styles.form}
                                onSubmit={(e) => {
                                  const form = fields(e);
                                  void send("kitchen.cancel", {
                                    id: f.id,
                                    expectedVersion: f.version,
                                    reason: text(form, "reason"),
                                  });
                                }}
                              >
                                <Field>
                                  Reason
                                  <TextInput name="reason" minLength={3} required />
                                </Field>
                                <Button type="submit" disabled={busy || !online}>
                                  Confirm cancellation
                                </Button>
                              </form>
                            </details>
                          )}
                        </section>
                      ))}
                    {!data.kitchen.length && (
                      <p className={styles.empty}>No kitchen items pending</p>
                    )}
                  </div>
                </FeatureWidget>
              )}</FrontLocalizedTree>;
}
