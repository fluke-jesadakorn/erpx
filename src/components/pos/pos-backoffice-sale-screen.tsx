"use client";
import dynamic from "next/dynamic";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { ActionAnchor as UiActionAnchor } from "@/components/ui/foundation";
import { Button as UiButton, Field, Select, TextInput } from "@/components/ui/foundation";
import { FeatureWidget as FeatureWidgetBase } from "@/components/features/feature-page";
import type { PosRow } from "@/server/modules/pos/public/pos-core";
import Link from "next/link";
import { useEffect, useRef, type ComponentProps, type FormEvent, type ReactNode } from "react";
import styles from "./pos.module.css";
import { SourceSupportLink } from "@/components/modules/source-support-link";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/front-language";
import { formatUiDateTime } from "@/lib/ui-format";
import type {BackofficePosConsoleContext} from "./pos-backoffice-console";


const RoomChargeReversalRequest = dynamic(() => import("@/components/hospitality/room-charge-reversals").then(module => module.RoomChargeReversalRequest));


const TaxDocumentReference = dynamic(() => import("@/components/hospitality/tax-document-reference").then(module => module.TaxDocumentReference));


const PosPromptPayEmailRecovery = dynamic(() => import("./pos-sales").then(module => module.PosPromptPayEmailRecovery));


const PosPendingTransfer = dynamic(() => import("@/components/pos/pos-pending-transfer").then(module => module.PosPendingTransfer));


type Row = PosRow;


const s = (r: Row, k: string) => String(r[k] ?? "");


const n = (r: Row, k: string) => Number(r[k] ?? 0);


const fields = (e: FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  return new FormData(e.currentTarget);
};


const text = (f: FormData, k: string) => String(f.get(k) ?? "");


const amount = (f: FormData, k: string) => Math.round(Number(f.get(k) ?? 0) * 100);


const quantity = (f: FormData, k: string) => Math.round(Number(f.get(k) ?? 0) * 1000);


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


function CheckoutQr({ url }: { url: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let active = true;
    const target = canvas.current;
    if (target) void import("qrcode").then(({default: QRCode}) => {
      if (active && canvas.current === target)
        return QRCode.toCanvas(target, url, { width: 220, margin: 4 });
    }).catch(() => {});
    return () => { active = false; };
  }, [url]);
  return (
    <FrontLocalizedTree>
      <div>
        <canvas ref={canvas} aria-label="Scan to open Stripe checkout" />
        <UiActionAnchor
          variant="primary"
          className={styles.button}
          href={url}
          target="_blank"
          rel="noreferrer"
        >
          Open customer checkout
        </UiActionAnchor>
      </div>
    </FrontLocalizedTree>
  );
}


function PromptPayQr({ data, expiresAt }: { data: string; expiresAt: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const language = useFrontLanguage();
  useEffect(() => {
    let active = true;
    const target = canvas.current;
    if (target) void import("qrcode").then(({default: QRCode}) => {
      if (active && canvas.current === target)
        return QRCode.toCanvas(target, data, { width: 260, margin: 4 });
    }).catch(() => {});
    return () => { active = false; };
  }, [data]);
  return (
    <FrontLocalizedTree>
      <div>
        <canvas ref={canvas} aria-label="Scan this PromptPay QR with a Thai banking app" />
        <p>
          {frontText("Scan with a Thai banking app · Valid until", language)}{" "}
          {formatUiDateTime(expiresAt, "Asia/Bangkok", { hour: "2-digit", minute: "2-digit" })}
        </p>
      </div>
    </FrontLocalizedTree>
  );
}


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
export function BackofficePosConsoleSaleScreen({context}: {context: BackofficePosConsoleContext}) {
  const {shift, send, register, registerId, setRegisterId, setSelected, data, busy, currentTabLoaded, can, setTab, sale, search, saleCommand, setSearch, category, setCategory, filtered, setProduct, money, product, online, erpBase, saleMoney, due, paymentMethod, setPaymentMethod, setPrinting, pickSale, currency} = context;
  return <FrontLocalizedTree>{(
                <>
                  <FeatureWidget id="pos-new-sale" title="New sale">
                    <form
                      className={styles.form}
                      onSubmit={(e) => {
                        const f = fields(e);
                        if (shift)
                          void send(
                            "sale.create",
                            {
                              shiftId: shift.id,
                              tableId: text(f, "table") || null,
                              partyId: text(f, "customer") || null,
                              guests: Number(f.get("guests") ?? 1),
                              serviceMode:
                                register?.mode === "restaurant" ? text(f, "service") : "retail",
                            },
                            true,
                          );
                      }}
                    >
                      <Field>
                        Register
                        <Select
                          aria-label="Register"
                          value={registerId}
                          onChange={(e) => {
                            setRegisterId(e.target.value);
                            setSelected("");
                          }}
                        >
                          <option value="">Select register</option>
                          {data.registers
                            .filter((r) => r.active)
                            .map((r) => (
                              <option key={s(r, "id")} value={s(r, "id")}>
                                {s(r, "name")}
                              </option>
                            ))}
                        </Select>
                      </Field>
                      {register?.mode === "restaurant" && (
                        <>
                          <Field>
                            Service mode
                            <Select name="service">
                              <option value="dine_in">Dine in</option>
                              <option value="takeaway">Takeaway</option>
                            </Select>
                          </Field>
                          <Field>
                            Table
                            <Select name="table">
                              <option value="">No table</option>
                              {data.tables
                                .filter((t) => t.register_id === registerId)
                                .map((t) => (
                                  <option key={s(t, "id")} value={s(t, "id")}>
                                    {s(t, "name")}
                                  </option>
                                ))}
                            </Select>
                          </Field>
                          <Field>
                            Guests
                            <TextInput name="guests" type="number" min="1" defaultValue="1" />
                          </Field>
                        </>
                      )}
                      <Field>
                        Customer
                        <Select name="customer">
                          <option value="">Walk-in customer</option>
                          {data.customers.map((c) => (
                            <option key={s(c, "id")} value={s(c, "id")}>
                              {s(c, "name")}
                            </option>
                          ))}
                        </Select>
                      </Field>
                      <Button
                        type="submit"
                        disabled={busy || !currentTabLoaded || !shift || !can("pos.sell")}
                      >
                        Open new sale
                      </Button>
                      {!shift && (
                        <Button secondary onClick={() => setTab("shifts")}>
                          Open a cash session before selling
                        </Button>
                      )}
                    </form>
                  </FeatureWidget>
                  <>
                    {data.settings?.enabled && currentTabLoaded && shift && sale && (
                      <FeatureWidget
                        id="pos-products-menu"
                        title="Products & menu"
                        className={styles.panel}
                      >
                        <form
                          className={styles.form}
                          onSubmit={(e) => {
                            e.preventDefault();
                            const p = data.products.find(
                              (p) => p.barcode === search || p.sku === search,
                            );
                            if (p && sale) {
                              void saleCommand("sale.add", {
                                productId: p.id,
                                quantityMilli: 1000,
                                modifierIds: [],
                                discountMinor: 0,
                                note: "",
                              });
                              setSearch("");
                            }
                          }}
                        >
                          <Field>
                            Search / scan barcode
                            <TextInput
                              aria-label="Search products"
                              value={search}
                              onChange={(e) => setSearch(e.target.value)}
                              placeholder="Product name, SKU or barcode"
                            />
                          </Field>
                          <Field>
                            Category
                            <Select value={category} onChange={(e) => setCategory(e.target.value)}>
                              <option value="">All categories</option>
                              {[
                                ...new Set(
                                  data.products.map((p) => s(p, "category")).filter(Boolean),
                                ),
                              ].map((c) => (
                                <option key={c}>{c}</option>
                              ))}
                            </Select>
                          </Field>
                        </form>
                        <div className={styles.products}>
                          {filtered.map((p) => {
                            const stock = data.stock.find(
                              (st) =>
                                st.product_id === p.id &&
                                st.warehouse_id === register?.warehouse_id,
                            );
                            return (
                              <UiButton
                                type="submit"
                                variant="plain"
                                disabled={
                                  !sale || busy || !can("pos.sell") || sale.state === "completed"
                                }
                                className={styles.product}
                                key={s(p, "id")}
                                onClick={() => setProduct(p)}
                              >
                                <span className={styles.muted}>{s(p, "sku")}</span>
                                <strong>{s(p, "name")}</strong>
                                <span>{money(n(p, "price_minor"))}</span>
                                <span className={styles.muted}>
                                  {stock && n(stock, "quality_held")
                                    ? "Quality hold — unavailable"
                                    : p.kind === "recipe"
                                      ? "Recipe item"
                                      : p.kind === "service"
                                        ? "Service"
                                        : `Available ${stock ? (n(stock, "quantity_milli") - n(stock, "reserved_milli")) / 1000 : 0} ${s(p, "unit")}`}
                                </span>
                              </UiButton>
                            );
                          })}
                        </div>
                        {!filtered.length && (
                          <p className={styles.empty}>No products match the search</p>
                        )}
                        {product && sale && (
                          <section className={`${styles.panel} ${styles.spaced}`}>
                            <h2>{s(product, "name")}</h2>
                            <form
                              className={styles.fields}
                              onSubmit={(e) => {
                                const f = fields(e);
                                void saleCommand("sale.add", {
                                  productId: product.id,
                                  quantityMilli: quantity(f, "qty"),
                                  modifierIds: f.getAll("modifier").map(String),
                                  discountMinor: amount(f, "discount"),
                                  note: text(f, "note"),
                                })?.then((r) => {
                                  if (r) setProduct(null);
                                });
                              }}
                            >
                              <Field>
                                Quantity
                                <TextInput
                                  name="qty"
                                  type="number"
                                  min="0.001"
                                  step="0.001"
                                  defaultValue="1"
                                  required
                                />
                              </Field>
                              <Field>
                                Line discount
                                <TextInput
                                  name="discount"
                                  type="number"
                                  min="0"
                                  step="0.01"
                                  defaultValue="0"
                                />
                              </Field>
                              <Field>
                                Notes
                                <TextInput name="note" maxLength={500} />
                              </Field>
                              {(
                                JSON.parse(s(product, "modifiers_json") || "[]") as Array<{
                                  id: string;
                                  name: string;
                                  priceMinor: number;
                                }>
                              ).map((m) => (
                                <Field orientation="inline" className={styles.check} key={m.id}>
                                  <TextInput type="checkbox" name="modifier" value={m.id} />
                                  {m.name} +{money(m.priceMinor)}
                                </Field>
                              ))}
                              <div className={styles.actions}>
                                <Button type="submit" disabled={busy || !online}>
                                  Add to sale
                                </Button>
                                <Button secondary onClick={() => setProduct(null)}>
                                  Close
                                </Button>
                              </div>
                            </form>
                          </section>
                        )}
                      </FeatureWidget>
                    )}
                    <FeatureWidget id="pos-cart-payment" title="Cart &amp; payment">
                      {!sale ? (
                        <div className={styles.empty}>Open a new sale or choose a held sale</div>
                      ) : (
                        <>
                          <div className={styles.header}>
                            <h2>{s(sale, "number")}</h2>
                            <Tag value={s(sale, "state")} />
                          </div>
                          <SourceSupportLink base={erpBase} kind="pos_sale" id={s(sale,"id")} version={n(sale,"version")}/>
                          {data.lines.map((line) => (
                            <div className={styles.row} key={s(line, "id")}>
                              <div>
                                <strong>{s(line, "name")}</strong>
                                <span className={styles.muted}>
                                  {n(line, "quantity_milli") / 1000} ×{" "}
                                  {saleMoney(n(line, "price_minor"))} {s(line, "note")}
                                </span>
                                <div>
                                  <Tag value={s(line, "fulfillment_state")} />
                                </div>
                              </div>
                              <div>
                                <strong>
                                  {saleMoney(n(line, "subtotal_minor") + n(line, "tax_minor"))}
                                </strong>
                                {["open", "held"].includes(s(sale, "state")) && can("pos.sell") && (
                                  <Button
                                    secondary
                                    disabled={busy || !online}
                                    onClick={() =>
                                      void saleCommand("sale.remove", { lineId: line.id })
                                    }
                                  >
                                    Remove
                                  </Button>
                                )}
                                {line.kind === "recipe" &&
                                  line.fulfillment_state === "draft" &&
                                  can("pos.sell") && (
                                    <Button
                                      disabled={busy || !online}
                                      onClick={() =>
                                        void send("kitchen.send", {
                                          id: line.fulfillment_id,
                                          expectedVersion: line.fulfillment_version,
                                        })
                                      }
                                    >
                                      Send to kitchen
                                    </Button>
                                  )}
                              </div>
                            </div>
                          ))}
                          <div className={styles.row}>
                            <span>Discount</span>
                            <span>{saleMoney(n(sale, "discount_minor"))}</span>
                          </div>
                          <div className={styles.row}>
                            <span>Tax</span>
                            <span>{saleMoney(n(sale, "tax_minor"))}</span>
                          </div>
                          <div className={styles.total}>
                            <span>Total</span>
                            <span>{saleMoney(n(sale, "total_minor"))}</span>
                          </div>
                          {sale.state !== "completed" &&
                            data.payments.some(
                              (p) => p.state === "succeeded" || p.provider_received_at,
                            ) && (
                              <div className={styles.alert}>
                                Payment received and pending — check the balance and close the
                                existing sale before starting another
                              </div>
                            )}
                          {data.payments.map((p) => (
                            <div className={styles.row} key={s(p, "id")}>
                              <div>
                                {labels[s(p, "method")]} · {saleMoney(n(p, "amount_minor"))}
                                <div>
                                  <Tag value={s(p, "state")} />
                                  {p.method === "cash" &&
                                    n(p, "tendered_minor") > n(p, "amount_minor") && (
                                      <span>
                                        {" "}
                                        Change{" "}
                                        {saleMoney(n(p, "tendered_minor") - n(p, "amount_minor"))}
                                      </span>
                                    )}
                                </div>
                              </div>
                              <PosPromptPayEmailRecovery
                                payment={p}
                                send={send}
                                busy={busy}
                                online={online}
                              />
                              {p.qr_data && p.state === "pending" && (
                                <PromptPayQr
                                  data={s(p, "qr_data")}
                                  expiresAt={n(p, "expires_at")}
                                />
                              )}
                              {p.checkout_url && p.state !== "succeeded" && (
                                <CheckoutQr url={s(p, "checkout_url")} />
                              )}
                              {s(p, "method").startsWith("stripe_") &&
                                ["pending", "requested", "exception"].includes(s(p, "state")) && (
                                  <Button
                                    secondary
                                    disabled={busy || !online}
                                    onClick={() =>
                                      void send("payment.cancel", {
                                        id: p.id,
                                        expectedVersion: p.version,
                                      })
                                    }
                                  >
                                    Check result / cancel wait
                                  </Button>
                                )}
                            </div>
                          ))}
                          {data.payments.filter(payment=>payment.method==='transfer'&&payment.state==='pending').map(payment=><PosPendingTransfer
                            key={s(payment,'id')} payment={payment} data={data} currency={s(sale,'currency')||data.currency}
                            canVerify={can('finance.approve')} canCancel={can('pos.sell')} send={send} busy={busy} online={online}
                          />)}
                          {data.roomChargeEnabled &&
                            can("pos.sell") &&
                            ["open", "held"].includes(s(sale, "state")) &&
                            data.payments.length === 0 &&
                            data.roomChargeStays.length > 0 && (
                              <form
                                className={styles.fields}
                                onSubmit={(e) => {
                                  const f = fields(e),
                                    stay = data.roomChargeStays.find(
                                      (st) => st.stay_id === text(f, "stay"),
                                    );
                                  if (stay)
                                    void saleCommand("sale.charge_to_room", {
                                      propertyId: stay.property_id,
                                      stayId: stay.stay_id,
                                      expectedPropertyVersion: stay.property_version,
                                    });
                                }}
                              >
                                <h3>Charge full bill to room</h3>
                                <Field>
                                  In-house guest and room
                                  <Select name="stay" required>
                                    <option value="">Select a checked-in stay</option>
                                    {data.roomChargeStays.map((st) => (
                                      <option key={s(st, "stay_id")} value={s(st, "stay_id")}>
                                        Room {s(st, "room_number")} · {s(st, "guest_name")}
                                      </option>
                                    ))}
                                  </Select>
                                </Field>
                                <p className={styles.muted}>
                                  Verify the room and guest before charging. The hotel guest account
                                  receives the full {saleMoney(n(sale, "total_minor"))}; the guest
                                  pays at checkout.
                                </p>
                                <Button type="submit" disabled={busy || !data.lines.length}>
                                  Charge to room
                                </Button>
                              </form>
                            )}
                          {can("pos.sell") &&
                            sale.state !== "completed" &&
                            sale.state !== "cancelled" && (
                              <>
                                <h3>Payment · Remaining {saleMoney(due)}</h3>
                                <form
                                  key={`${sale.id}:${due}`}
                                  className={styles.fields}
                                  onSubmit={(e) => {
                                    const f = fields(e),
                                      value = amount(f, "amount");
                                    void saleCommand("payment.create", {
                                      method: text(f, "method"),
                                      amountMinor: value,
                                      tenderedMinor: text(f, "tender")
                                        ? amount(f, "tender")
                                        : value,
                                      reference: text(f, "reference"),
                                      payerEmail: text(f, "payerEmail"),
                                    });
                                  }}
                                >
                                  <Field>
                                    Method
                                    <Select
                                      name="method"
                                      value={paymentMethod}
                                      onChange={(e) => setPaymentMethod(e.target.value)}
                                    >
                                      {data.availableTenderMethods.map((m) => (
                                        <option key={m} value={m}>
                                          {labels[m]}
                                        </option>
                                      ))}
                                    </Select>
                                  </Field>
                                  <Field>
                                    Payment amount
                                    <TextInput
                                      name="amount"
                                      type="number"
                                      min="0.01"
                                      step="0.01"
                                      max={due / 100}
                                      defaultValue={(due / 100).toFixed(2)}
                                      required
                                    />
                                  </Field>
                                  <Field>
                                    Cash received (leave blank for exact amount)
                                    <TextInput name="tender" type="number" min="0" step="0.01" />
                                  </Field>
                                  <Field>
                                    Transfer reference
                                    <TextInput name="reference" maxLength={150} />
                                  </Field>
                                  <Field>
                                    Payer email (required for PromptPay payment and refund)
                                    <TextInput
                                      name="payerEmail"
                                      type="email"
                                      required={paymentMethod === "stripe_promptpay"}
                                    />
                                  </Field>
                                  <Button type="submit" disabled={busy || due <= 0}>
                                    Record payment / create QR
                                  </Button>
                                </form>
                                <div className={`${styles.actions} ${styles.spaced}`}>
                                  <Button
                                    disabled={busy || due > 0 || !data.lines.length}
                                    onClick={() => void saleCommand("sale.complete")}
                                  >
                                    Confirm and close sale
                                  </Button>
                                  <Button
                                    secondary
                                    disabled={busy || !online}
                                    onClick={() =>
                                      void saleCommand(
                                        sale.state === "held" ? "sale.resume" : "sale.hold",
                                      )
                                    }
                                  >
                                    {sale.state === "held" ? "Resume sale" : "Held"}
                                  </Button>
                                  <Button
                                    secondary
                                    disabled={busy || !online}
                                    onClick={() => void saleCommand("sale.cancel")}
                                  >
                                    Cancel sale
                                  </Button>
                                </div>
                              </>
                            )}
                          {sale.state === "completed" && (
                            <div className={styles.actions}>
                              <Button
                                onClick={() => {
                                  setPrinting(true);
                                  setTimeout(() => window.print(), 150);
                                }}
                              >
                                Print receipt
                              </Button>
                              {sale.invoice_id && (
                                <Link href={`${erpBase}/finance/invoices/${sale.invoice_id}`}>
                                  Accounting document
                                </Link>
                              )}
                              {data.roomCharge && (
                                <span>
                                  {data.roomCharge.reversal_entry_id
                                    ? "Reversed hotel charge"
                                    : "Charged to hotel guest account"}{" "}
                                  · Entry {s(data.roomCharge, "guest_account_entry_id").slice(0, 8)}
                                </span>
                              )}
                            </div>
                          )}
                          {sale.state === "completed" && sale.invoice_id && (
                            <TaxDocumentReference
                              base={erpBase}
                              invoiceId={s(sale, "invoice_id")}
                            />
                          )}
                          {data.roomCharge && (
                            <>
                              {data.roomCharge.reversal_entry_id ? (
                                <div className={styles.alert}>
                                  Room charge reversed · Hotel adjustment{" "}
                                  {s(data.roomCharge, "reversal_entry_id").slice(0, 8)} ·{" "}
                                  {s(data.roomCharge, "reversal_reason")}
                                </div>
                              ) : (
                                <>
                                  <RoomChargeReversalRequest data={data} send={send} busy={busy}/>
                                </>
                              )}
                            </>
                          )}
                          {can("pos.approve") && sale.state !== "completed" && (
                            <details className={styles.spaced}>
                              <summary>Approve discount</summary>
                              <form
                                className={styles.form}
                                onSubmit={(e) => {
                                  const f = fields(e);
                                  void saleCommand("sale.approve_discount", {
                                    reason: text(f, "reason"),
                                  });
                                }}
                              >
                                <Field>
                                  Reason
                                  <TextInput name="reason" minLength={3} required />
                                </Field>
                                <Button type="submit" disabled={busy || !online}>
                                  Approve discount
                                </Button>
                              </form>
                            </details>
                          )}
                          {["open", "held"].includes(s(sale, "state")) && (
                            <FormDisclosure className={styles.spaced}>
                              <summary>Move table / merge / split sale</summary>
                              <form
                                className={styles.form}
                                onSubmit={(e) => {
                                  const f = fields(e);
                                  void saleCommand("sale.move_table", {
                                    tableId: text(f, "table"),
                                  });
                                }}
                              >
                                <Field>
                                  Move to table
                                  <Select name="table">
                                    {data.tables
                                      .filter((t) => t.register_id === sale.register_id)
                                      .map((t) => (
                                        <option key={s(t, "id")} value={s(t, "id")}>
                                          {s(t, "name")}
                                        </option>
                                      ))}
                                  </Select>
                                </Field>
                                <Button type="submit" disabled={busy || !online}>
                                  Move table
                                </Button>
                              </form>
                              <form
                                className={styles.form}
                                onSubmit={(e) => {
                                  const f = fields(e),
                                    target = data.sales.find((s) => s.id === text(f, "target"));
                                  if (target)
                                    void saleCommand("sale.merge", {
                                      targetId: target.id,
                                      targetVersion: target.version,
                                    });
                                }}
                              >
                                <Field>
                                  Merge into sale
                                  <Select name="target">
                                    {data.sales
                                      .filter(
                                        (t) =>
                                          t.id !== sale.id &&
                                          t.register_id === sale.register_id &&
                                          ["open", "held"].includes(s(t, "state")),
                                      )
                                      .map((t) => (
                                        <option key={s(t, "id")} value={s(t, "id")}>
                                          {s(t, "number")}
                                        </option>
                                      ))}
                                  </Select>
                                </Field>
                                <Button type="submit" disabled={busy || !online}>
                                  Merge sales
                                </Button>
                              </form>
                              <form
                                className={styles.form}
                                onSubmit={(e) => {
                                  const f = fields(e);
                                  void send(
                                    "sale.split",
                                    {
                                      id: sale.id,
                                      expectedVersion: sale.version,
                                      lineId: text(f, "line"),
                                      quantityMilli: quantity(f, "qty"),
                                    },
                                    true,
                                  );
                                }}
                              >
                                <Field>
                                  Line to split
                                  <Select name="line">
                                    {data.lines.map((l) => (
                                      <option key={s(l, "id")} value={s(l, "id")}>
                                        {s(l, "name")}
                                      </option>
                                    ))}
                                  </Select>
                                </Field>
                                <Field>
                                  Quantity
                                  <TextInput
                                    name="qty"
                                    type="number"
                                    step="0.001"
                                    min="0.001"
                                    defaultValue="1"
                                  />
                                </Field>
                                <Button type="submit" disabled={busy || !online}>
                                  Split into new sale
                                </Button>
                              </form>
                            </FormDisclosure>
                          )}
                          {["completed", "payment_pending"].includes(s(sale, "state")) &&
                            !data.roomCharge &&
                            can("pos.sell") && (
                              <FormDisclosure className={styles.spaced}>
                                <summary>Request return / refund</summary>
                                <form
                                  className={styles.fields}
                                  onSubmit={(e) => {
                                    const f = fields(e);
                                    void saleCommand("refund.request", {
                                      reason: text(f, "reason"),
                                      payoutShiftId: text(f, "shift") || undefined,
                                      ...(sale.state === "completed"
                                        ? {
                                            lines:
                                              text(f, "line") === "all"
                                                ? data.lines
                                                    .filter(
                                                      (l) =>
                                                        n(l, "returned_milli") <
                                                        n(l, "quantity_milli"),
                                                    )
                                                    .map((l) => ({
                                                      lineId: l.id,
                                                      quantityMilli:
                                                        n(l, "quantity_milli") -
                                                        n(l, "returned_milli"),
                                                      restock:
                                                        f.get("restock") === "on" &&
                                                        l.kind === "stock",
                                                    }))
                                                : [
                                                    {
                                                      lineId: text(f, "line"),
                                                      quantityMilli: quantity(f, "qty"),
                                                      restock: f.get("restock") === "on",
                                                    },
                                                  ],
                                          }
                                        : { amountMinor: amount(f, "refund") }),
                                    });
                                  }}
                                >
                                  {sale.state === "completed" ? (
                                    <>
                                      <Field>
                                        Line
                                        <Select name="line">
                                          <option value="all">
                                            Return whole sale (unrefunded items only)
                                          </option>
                                          {data.lines
                                            .filter(
                                              (l) =>
                                                n(l, "returned_milli") < n(l, "quantity_milli"),
                                            )
                                            .map((l) => (
                                              <option key={s(l, "id")} value={s(l, "id")}>
                                                {s(l, "name")} · Returnable{" "}
                                                {(n(l, "quantity_milli") - n(l, "returned_milli")) /
                                                  1000}
                                              </option>
                                            ))}
                                        </Select>
                                      </Field>
                                      <Field>
                                        Return quantity
                                        <TextInput
                                          name="qty"
                                          type="number"
                                          min="0.001"
                                          step="0.001"
                                          defaultValue="1"
                                        />
                                      </Field>
                                      <Field orientation="inline" className={styles.check}>
                                        <TextInput name="restock" type="checkbox" />
                                        Restock resalable items
                                      </Field>
                                    </>
                                  ) : (
                                    <Field>
                                      Advance payment to refund
                                      <TextInput
                                        name="refund"
                                        type="number"
                                        min="0.01"
                                        step="0.01"
                                        required
                                      />
                                    </Field>
                                  )}
                                  <Field>
                                    Cash refund session
                                    <Select name="shift">
                                      <option value="">No cash refund</option>
                                      {data.shifts
                                        .filter(
                                          (sh) =>
                                            sh.state === "open" && sh.branch_id === sale.branch_id,
                                        )
                                        .map((sh) => (
                                          <option key={s(sh, "id")} value={s(sh, "id")}>
                                            {s(sh, "register_name")}
                                          </option>
                                        ))}
                                    </Select>
                                  </Field>
                                  <Field>
                                    Reason
                                    <TextInput name="reason" required minLength={3} />
                                  </Field>
                                  <Button type="submit" disabled={busy || !online}>
                                    Send request to manager
                                  </Button>
                                </form>
                              </FormDisclosure>
                            )}
                        </>
                      )}
                    </FeatureWidget>
                  </>
                  <FeatureWidget id="pos-open-sales" title="Open sales" className={styles.panel}>
                    <div className={styles.actions}>
                      {data.sales
                        .filter((row) =>
                          ["open", "held", "payment_pending"].includes(s(row, "state")),
                        )
                        .map((row) => (
                          <Button secondary key={s(row, "id")} onClick={() => pickSale(row)}>
                            {s(row, "number")} ·{" "}
                            {money(
                              n(row, "total_minor"),
                              s(row, "currency") || currency,
                              s(row, "business_date"),
                            )}
                          </Button>
                        ))}
                    </div>
                  </FeatureWidget>
                </>
              )}</FrontLocalizedTree>;
}
