"use client";
import { PosRecordDetail } from "@/components/records/pos-record-detail";
import { RecordDrawer } from "@/components/records/record-drawer";
import { RecordLink } from "@/components/records/record-link";
import { recordCanonicalHref } from "@/lib/record-drawer";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import {
  ActionAnchor as UiActionAnchor,
  ActionLink as UiActionLink,
} from "@/components/ui/foundation";

import {
  Button as UiButton,
  Field,
  Select,
  Table,
  TableFrame,
  TextInput,
} from "@/components/ui/foundation";

import { FeatureWidget as FeatureWidgetBase, WidgetGrid } from "@/components/features/feature-page";

import { ListPagination } from "@/components/ui/list-pagination";
import { SetupChecklist } from "@/components/ui/setup-checklist";
import { TaxDocumentReference } from "@/components/hospitality/tax-document-reference";
import { accountKeys, type PosAccounts } from "@/lib/pos";
import { useMoneyFormatter } from "@/components/money/currency-provider";
import { posCommerceAccountDefaults, posGeneratedAccountSpecs } from "@/lib/pos-bootstrap";
import { workFetch } from "@/lib/work-fetch";
import type { PosWorkspace } from "@/server/pos";
import type { PosRow } from "@/server/pos-core";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import QRCode from "qrcode";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ComponentProps,
  type FormEvent,
  type ReactNode,
} from "react";
import styles from "./pos.module.css";
import { PosSaleWorkspace, PosReceipt, PosPromptPayEmailRecovery, PosPendingTransfer } from "./pos-sales";
import {SourceSupportLink} from "@/components/modules/source-support-link";
import { PosDayClosePanel, CashDenominationCount } from "./pos-closing";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/front-language";
import { formatUiDateTime } from "@/lib/ui-format";
import { AttachmentPanel } from "@/components/attachments/attachment-panel";
import { ModuleTiles } from "@/components/hospitality/module-tiles";
import { posMenu } from "@/components/hospitality/module-menu";
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
    if (canvas.current)
      void QRCode.toCanvas(canvas.current, url, { width: 220, margin: 4 }).catch(() => {});
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
    if (canvas.current)
      void QRCode.toCanvas(canvas.current, data, { width: 260, margin: 4 }).catch(() => {});
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
function subscribePosConnectivity(onChange: () => void) {
  window.addEventListener("online", onChange);
  window.addEventListener("offline", onChange);
  return () => {
    window.removeEventListener("online", onChange);
    window.removeEventListener("offline", onChange);
  };
}
const posConnectivitySnapshot = () => navigator.onLine;
const posServerConnectivitySnapshot = () => true;
type PosSend = (
  op: string,
  input: unknown,
  select?: boolean,
) => Promise<Record<string, unknown> | undefined>;

function FrontPosDaily({
  data,
  tab,
  setTab,
  busy,
  error,
  notice,
  send,
  refresh,
  registerId,
  setSelected,
  setRegisterId,
  money,
  can,
  approvalHref,
  erpBase,
  api,
  recovery,
  loading,
  online,
}: {
  data: PosWorkspace;
  tab: string;
  setTab: (value: string) => void;
  busy: boolean;
  error: string;
  notice: string;
  send: PosSend;
  refresh: (saleId?: string) => Promise<void>;
  registerId: string;
  setSelected: (value: string) => void;
  setRegisterId: (value: string) => void;
  money: (value: number, recordCurrency?: string | null, date?: string) => string;
  can: (permission: string) => boolean;
  approvalHref: string;
  erpBase: string;
  api: string;
  recovery: ReactNode;
  loading: boolean;
  online: boolean;
}) {
  const language = useFrontLanguage(),
    tr = (value: string) => frontText(value, language);
  const [kitchenStation, setKitchenStation] = useState("");
  const sale = data.sale,
    activeRegisterId =
      registerId ||
      s(sale ?? {}, "register_id") ||
      s(data.registers.find((row) => Boolean(row.active)) ?? {}, "id");
  const register = data.registers.find((row) => s(row, "id") === activeRegisterId);
  const ready = Boolean(
    data.frontReady ??
    (data.settings?.enabled &&
      data.registers.some((row) => Boolean(row.active)) &&
      data.products.length),
  );
  const tabs = [
    ["sale", tr("Sale"), can("pos.sell")],
    ["kitchen", tr("Kitchen"), can("pos.kitchen")],
    ["shifts", tr("Cash shifts"), can("pos.sell") || can("pos.approve")],
    ["history", tr("History"), can("pos.read")],
  ].filter((item) => item[2]) as Array<[string, string, boolean]>;
  const pick = (row: Row) => {
    setSelected(s(row, "id"));
    setRegisterId(s(row, "register_id"));
    const url = new URL(window.location.href); url.searchParams.set("detailKind", "pos_sale"); url.searchParams.set("detailId", s(row,"id")); window.history.pushState(null,"",url);
  };
  return (
    <FrontLocalizedTree>
      <div className={styles.workspace} data-source="live" data-front-daily="pos">
        <header className={styles.header}>
          <div>
            <p className={styles.eyebrow}>{tr("Daily work")}</p>
            <h1>{tr("Point of sale")}</h1>
            <p className={styles.muted}>{tr("Sell, serve, and close the cash shift")}</p>
          </div>
          <Button secondary onClick={() => void refresh().catch(() => {})}>
            {tr("Refresh")}
          </Button>
        </header>
        <ModuleTiles
          label={tr("Point of sale")}
          current={tab}
          items={posMenu(language).filter((item) => tabs.some(([id]) => id === item.id))}
          onSelect={setTab}
        />
        {error && (
          <div className={`${styles.alert} ${styles.error}`} role="alert">
            {tr(error)}
          </div>
        )}
        {notice && (
          <div className={`${styles.alert} ${styles.success}`} role="status">
            {tr(notice)}
          </div>
        )}
        {recovery}
        {loading ? (
          <section className={styles.panel} role="status" aria-busy="true">
            <p className={styles.empty}>
              {language === "th" ? "กำลังโหลดข้อมูลจุดขาย…" : "Loading the point of sale…"}
            </p>
          </section>
        ) : !ready ? (
          <section className={styles.setupCard} role="status">
            <h2>{tr("Not ready for daily work")}</h2>
            <p>
              {language === "th"
                ? "จุดขายยังไม่พร้อมเปิดงาน แจ้งผู้ดูแล POS พร้อมชื่อเครื่องหรือสาขาที่กำลังใช้งาน"
                : "This register is not ready. Contact the POS administrator with this register or location name."}
            </p>
            {register && (
              <p>
                {language === "th" ? "จุดขาย" : "Register"}: {s(register, "name")} (
                {s(register, "id").slice(0, 8)})
              </p>
            )}
            <ul>
              {!data.settings?.enabled && (
                <li>
                  {language === "th"
                    ? "ผู้ดูแล POS: เปิดใช้งานการขายและตั้งค่าบัญชี"
                    : "POS administrator: enable sales and accounting setup"}
                </li>
              )}
              {!data.registers.some((row) => Boolean(row.active)) && (
                <li>
                  {language === "th"
                    ? "ผู้ดูแล POS: เปิดจุดขายที่ใช้งานได้"
                    : "POS administrator: activate a register"}
                </li>
              )}
              {!(data.frontSetup?.hasProducts ?? Boolean(data.products.length)) && (
                <li>
                  {language === "th"
                    ? "ผู้ดูแลสินค้า: เพิ่มสินค้าที่ขายได้"
                    : "Product administrator: add an active product"}
                </li>
              )}
              {data.frontSetup?.blockedRegisters
                .filter((row) => !register || row.register_id === register.id)
                .map((row) => (
                  <li key={s(row, "register_id")}>
                    {row.hotel_state === "setup"
                      ? language === "th"
                        ? `${row.finance_approved ? "ผู้ดูแลโรงแรม: เปิดโรงแรมเพื่อใช้งานประจำวัน" : "ฝ่ายการเงิน: ให้ผู้อนุมัติอีกคนอนุมัติการตั้งค่า แล้วให้ผู้ดูแลเปิดโรงแรม"} · ${s(row, "hotel_name")}`
                        : `${row.finance_approved ? "Hotel administrator: open the hotel for daily work" : "Finance: have another person approve the setup, then ask the hotel administrator to open the hotel"} · ${s(row, "hotel_name")}`
                      : language === "th"
                        ? `ผู้ดูแลโรงแรม: ปิดวันเดิมและเปิดวันทำงานปัจจุบัน · ${s(row, "hotel_name")}`
                        : `Hotel administrator: finish the previous business day and open the current day · ${s(row, "hotel_name")}`}
                  </li>
                ))}
            </ul>
          </section>
        ) : (
          <>
            {tab === "sale" && can("pos.sell") && (
              <PosSaleWorkspace
                data={data}
                api={api}
                erpBase={erpBase}
                busy={busy}
                send={send}
                registerId={activeRegisterId}
                setRegisterId={(id) => {
                  setRegisterId(id);
                  setSelected("");
                }}
                pickSale={pick}
                openShifts={() => setTab("shifts")}
              />
            )}
            {tab === "kitchen" && can("pos.kitchen") && (
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
                        {(
                          {
                            draft: "kitchen.send",
                            queued: "kitchen.start",
                            preparing: "kitchen.ready",
                            ready: "kitchen.deliver",
                          } as Record<string, string>
                        )[s(item, "state")] && (
                          <Button
                            disabled={busy || !online}
                            onClick={() =>
                              void send(
                                (
                                  {
                                    draft: "kitchen.send",
                                    queued: "kitchen.start",
                                    preparing: "kitchen.ready",
                                    ready: "kitchen.deliver",
                                  } as Record<string, string>
                                )[s(item, "state")],
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
            )}
            {tab === "shifts" && (can("pos.sell") || can("pos.approve")) && (
              <section className={styles.panel}>
                <h2>{tr("Cash shifts")}</h2>
                {can("pos.sell") && (
                  <form
                    className={styles.form}
                    onSubmit={(event) => {
                      const f = fields(event);
                      void send("shift.open", {
                        registerId: text(f, "register") || activeRegisterId,
                        openingMinor: amount(f, "opening"),
                      });
                    }}
                  >
                    <Field>
                      {tr("Register")}
                      <Select name="register" defaultValue={activeRegisterId}>
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
                        defaultValue="0"
                        required
                      />
                    </Field>
                    <Button type="submit" disabled={busy || !online}>
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
            )}
            {tab === "history" && can("pos.read") && (
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
            )}
          </>
        )}
      </div>
    </FrontLocalizedTree>
  );
}

export function PosConsole({
  initial,
  base,
  erpBase = base,
  initialTab,
  frontMode = false,
  commandScopeKey = base,
  frontHref,
  recordOnly = false,
}: {
  initial: PosWorkspace;
  base: string;
  erpBase?: string;
  initialTab?: string;
  frontMode?: boolean;
  commandScopeKey?: string;
  frontHref?: string;
  recordOnly?: boolean;
}) {
  const formatSelectedMoney = useMoneyFormatter();
  const online = useSyncExternalStore(
    subscribePosConnectivity,
    posConnectivitySnapshot,
    posServerConnectivitySnapshot,
  );
  const language = useFrontLanguage(),
    tr = (value: string) => frontText(value, language);
  const searchParams = useSearchParams(),
    requestedTab = searchParams.get("tab") ?? initialTab ?? "sale";
  const maySell = Boolean(initial.permissions["pos.sell"]),
    mayKitchen = Boolean(initial.permissions["pos.kitchen"]),
    mayReview = Boolean(initial.permissions["pos.approve"]);
  const allowedDailyTab =
    requestedTab === "sale"
      ? maySell
      : requestedTab === "kitchen"
        ? mayKitchen
        : requestedTab === "shifts"
          ? maySell || mayReview
          : requestedTab === "history";
  const tab =
    frontMode && !allowedDailyTab
      ? maySell
        ? "sale"
        : mayKitchen
          ? "kitchen"
          : mayReview
            ? "shifts"
            : "history"
      : requestedTab;
  const setTab = (value: string) => {
    if (value !== tab) setLoadedTab("");
    const url = new URL(window.location.href);
    url.searchParams.set("tab", value);
    url.searchParams.delete("cursor");
    window.history.pushState(null, "", url);
  };
  const [data, setData] = useState(initial),
    [loadedTab, setLoadedTab] = useState(tab),
    [selected, setSelectedState] = useState(searchParams.get("detailKind") === "pos_sale" ? searchParams.get("detailId") ?? "" : initial.sale ? s(initial.sale, "id") : ""),
    [registerId, setRegisterId] = useState(
      initial.sale ? s(initial.sale, "register_id") : s(initial.registers[0] ?? {}, "id"),
    ),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [busy, setBusy] = useState(false),
    [search, setSearch] = useState(""),
    [category, setCategory] = useState(""),
    [product, setProduct] = useState<Row | null>(null),
    [printing, setPrinting] = useState(false),
    [kitchenStation, setKitchenStation] = useState(""),
    [paymentMethod, setPaymentMethod] = useState("cash");
  const setSelected = (value: string) => {
    if (value !== selected) setLoadedTab("");
    setSelectedState(value);
    if (frontMode) {
      const url = new URL(window.location.href);
      if (value) url.searchParams.set("sale", value);
      else url.searchParams.delete("sale");
      window.history.replaceState(null, "", url);
    }
  };
  const requestedSale = searchParams.get("detailKind") === "pos_sale" ? searchParams.get("detailId") : searchParams.get("sale"),
    priorSaleQuery = useRef(requestedSale);
  useEffect(() => {
    if (priorSaleQuery.current === requestedSale) return;
    priorSaleQuery.current = requestedSale;
    let cancelled = false;
    queueMicrotask(() => {
      if (!cancelled && (requestedSale ?? "") !== selected) {
        setLoadedTab("");
        setSelectedState(requestedSale ?? "");
      }
    });
    return () => {
      cancelled = true;
    };
  }, [frontMode, requestedSale, selected]);
  const [deviceReady, setDeviceReady] = useState(false);
  const deviceRegisterKey = "erpx-pos-register:" + data.userId + ":" + base + ":" + commandScopeKey;
  useEffect(() => {
    if (!frontMode) return;
    queueMicrotask(() => {
      try {
        const saved = localStorage.getItem(deviceRegisterKey);
        if (saved && initial.registers.some((item) => item.id === saved && item.active))
          setRegisterId(saved);
      } catch {}
      setDeviceReady(true);
    });
  }, [deviceRegisterKey, frontMode, initial.registers]);
  useEffect(() => {
    if (!frontMode || !deviceReady || !registerId) return;
    try {
      localStorage.setItem(deviceRegisterKey, registerId);
    } catch {}
  }, [deviceReady, deviceRegisterKey, frontMode, registerId]);
  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(""), 5000);
    return () => window.clearTimeout(timer);
  }, [notice]);
  type PendingCommand = {
    op: string;
    input: unknown;
    select: boolean;
    fingerprint: string;
    key: string;
  };
  const commandRef = useRef<PendingCommand | null>(null),
    busyRef = useRef(false),
    selectionRef = useRef(selected),
    loadingRef = useRef(false);
  const [pendingCommand, setPendingCommand] = useState<PendingCommand | null>(null);
  const commandStorageKey = "pos-command:" + data.userId + ":" + base + ":" + commandScopeKey;
  useEffect(() => {
    queueMicrotask(() => {
      try {
        const prior = JSON.parse(sessionStorage.getItem(commandStorageKey) ?? "null");
        if (
          prior &&
          typeof prior.op === "string" &&
          typeof prior.key === "string" &&
          "input" in prior
        ) {
          commandRef.current = prior;
          setPendingCommand(prior);
        } else {
          commandRef.current = null;
          setPendingCommand(null);
        }
      } catch {
        commandRef.current = null;
        setPendingCommand(null);
      }
    });
  }, [commandStorageKey]);
  const cursor = searchParams.get("cursor") ?? "",
    historyQuery = searchParams.get("q") ?? "";
  const refreshGeneration = useRef(0);
  const api = `/api/v1${base}/pos`,
    currency = data.currency;
  const money = (value: number, recordCurrency: string | null = currency, date?: string) =>
    formatSelectedMoney(value, recordCurrency || currency || "USD", date, 2);
  const saleMoney = (value: number) =>
    money(
      value,
      data.sale ? s(data.sale, "currency") : currency,
      data.sale ? s(data.sale, "business_date") : undefined,
    );
  const refresh = useCallback(
    async (saleId = selectionRef.current) => {
      const generation = ++refreshGeneration.current;
      const query = frontMode
        ? (new URL(window.location.href).searchParams.get("q") ?? "")
        : historyQuery;
      const r = await workFetch(
        `${api}?${new URLSearchParams({ ...(saleId ? { sale: saleId } : {}), q: query, tab, cursor })}`,
        { cache: "no-store" },
      );
      if (!r.headers.get("content-type")?.includes("application/json"))
        throw new Error("โหลดข้อมูล POS ไม่สำเร็จ · POS unavailable. กรุณากด Refresh อีกครั้ง.");
      const body = await r.json();
      if (!r.ok) throw new Error(body.error?.message ?? "Failed to load POS");
      if (generation === refreshGeneration.current && saleId === selectionRef.current) {
        setData(body.data);
        setLoadedTab(tab);
        setError((previous) =>
          [
            "โหลดข้อมูล POS ไม่สำเร็จ กรุณากดรีเฟรชอีกครั้ง",
            "POS data is temporarily unavailable. Try Refresh.",
            "Failed to load POS",
            "โหลดข้อมูล POS ไม่สำเร็จ · POS unavailable. กรุณากด Refresh อีกครั้ง.",
          ].includes(previous)
            ? ""
            : previous,
        );
      }
    },
    [api, tab, cursor, historyQuery, frontMode],
  );
  useEffect(
    () => () => {
      ++refreshGeneration.current;
    },
    [refresh],
  );
  useEffect(() => {
    selectionRef.current = selected;
    void refresh(selected).catch((e) => setError(e.message));
  }, [selected, refresh]);
  useEffect(() => {
    const reload = () => {
      if (!busyRef.current && !loadingRef.current && document.visibilityState === "visible") {
        loadingRef.current = true;
        void refresh()
          .catch((e) => setError(e.message))
          .finally(() => {
            loadingRef.current = false;
          });
      }
    };
    const timer = setInterval(reload, 5000);
    window.addEventListener("focus", reload);
    window.addEventListener("online", reload);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", reload);
      window.removeEventListener("online", reload);
    };
  }, [refresh]);
  async function send(op: string, input: unknown, select = false) {
    if (!navigator.onLine) {
      setError(
        language === "th"
          ? "เชื่อมต่ออินเทอร์เน็ตก่อนส่งครัว รับเงิน หรือปิดยอด"
          : "Reconnect before kitchen, payment or closing actions.",
      );
      return undefined;
    }
    if (op === "sale.create")
      input = {
        ...(input as Record<string, unknown>),
        currency: (input as Record<string, unknown>).currency ?? data.currency,
      };
    if (busyRef.current) return;
    const fingerprint = op + JSON.stringify(input);
    if (commandRef.current && commandRef.current.fingerprint !== fingerprint) {
      setError("มีคำสั่งที่ยังไม่ทราบผล กรุณาตรวจผลคำสั่งเดิมก่อนทำรายการต่อ");
      return;
    }
    const pending = commandRef.current ?? {
      op,
      input,
      select,
      fingerprint,
      key: crypto.randomUUID(),
    };
    commandRef.current = pending;
    try {
      sessionStorage.setItem(commandStorageKey, JSON.stringify(pending));
    } catch {}
    busyRef.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    const clear = () => {
      commandRef.current = null;
      setPendingCommand(null);
      try {
        sessionStorage.removeItem(commandStorageKey);
      } catch {}
    };
    try {
      const r = await workFetch(`${api}/${pending.op.replace(".", "/")}`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": pending.key },
          body: JSON.stringify(pending.input),
        }),
        body = await r.json();
      if (!r.ok) {
        if (r.status < 500) clear();
        throw new Error(body.error?.message ?? "Save failed");
      }
      clear();
      if (body.data.url) {
        window.location.assign(body.data.url);
        return;
      }
      if (pending.select) {
        selectionRef.current = body.data.id;
        setSelected(body.data.id);
        setTab("sale");
      }
      await refresh(pending.select ? body.data.id : selectionRef.current).catch((e) =>
        setError(e.message),
      );
      if (op === "settings.bootstrap" && typeof body.data.registerId === "string")
        setRegisterId(body.data.registerId);
      setNotice(
        op === "settings.bootstrap"
          ? `${body.data.mode === "restaurant" ? "Restaurant" : "Retail"} POS is set up. Open a cash session to start selling.`
          : "Saved",
      );
      return body.data;
    } catch (e) {
      if (commandRef.current) setPendingCommand(commandRef.current);
      setError(e instanceof Error ? e.message : "Save failed");
      return undefined;
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  const currentTabLoaded = loadedTab === tab;
  const can = (p: string) => Boolean(data.permissions[p]),
    sale = data.sale,
    register = data.registers.find((r) => r.id === registerId),
    shift = data.shifts.find((sh) => sh.register_id === registerId && sh.state === "open");
  const dailyPosRoot = frontHref ?? `${base}/pos`;
  const due = sale
    ? Math.max(
        0,
        n(sale, "total_minor") -
          data.payments
            .filter((p) => s(p, "state") === "succeeded")
            .reduce((a, p) => a + n(p, "amount_minor") - n(p, "refunded_minor"), 0),
      )
    : 0;
  const saleCommand = (op: string, extra: Record<string, unknown> = {}) =>
    sale ? send(op, { id: sale.id, expectedVersion: sale.version, ...extra }) : undefined;
  const pickSale = (row: Row) => {
    setSelected(s(row, "id"));
    setRegisterId(s(row, "register_id"));
    const url = new URL(window.location.href); url.searchParams.set("detailKind", "pos_sale"); url.searchParams.set("detailId", s(row,"id")); window.history.pushState(null,"",url);
    setPrinting(false);
  };
  const filtered = data.products.filter(
    (p) =>
      (!category || p.category === category) &&
      `${p.name} ${p.sku} ${p.barcode ?? ""}`.toLowerCase().includes(search.toLowerCase()),
  );
  const recovery = pendingCommand && (
    <div className={styles.alert} role="status">
      <p>{tr("คำสั่งเดิมยังไม่ทราบผล ตรวจด้วยหมายเลขเดิมก่อนทำรายการต่อ")}</p>
      <Button
        disabled={busy}
        onClick={() => void send(pendingCommand.op, pendingCommand.input, pendingCommand.select)}
      >
        ตรวจผลคำสั่งเดิม / Retry safely
      </Button>
    </div>
  );
  const receiptDrawer = searchParams.get("detailKind") === "pos_sale" && searchParams.get("detailId") ? <RecordDrawer title={data.sale ? s(data.sale,"number") : "Receipt"} fullHref={`?tab=sale&sale=${encodeURIComponent(searchParams.get("detailId")!)}`}>
    {error && <p role="alert">{error}<Button onClick={()=>void refresh().catch(()=>{})}>Retry</Button></p>}
    {data.sale && s(data.sale,"id") === searchParams.get("detailId") ? <PosRecordDetail key={s(data.sale,"id")} canEdit={can("pos.sell")} data={data} api={api} erpBase={erpBase} busy={busy} send={send} registerId={registerId} setRegisterId={setRegisterId} pickSale={pickSale} openShifts={()=>setTab("shifts")}/> : <p role="status">Loading receipt…</p>}
  </RecordDrawer> : null;
  if (recordOnly) return <div className="ui-stack">{error && <p role="alert">{error}<Button onClick={()=>void refresh().catch(()=>{})}>Retry</Button></p>}{data.sale ? <PosRecordDetail key={s(data.sale,"id")} canEdit={can("pos.sell")} data={data} api={api} erpBase={erpBase} busy={busy} send={send} registerId={registerId} setRegisterId={setRegisterId} pickSale={pickSale} openShifts={()=>setTab("shifts")}/> : <p role="alert">Receipt unavailable in this work scope.</p>}</div>;
  if (frontMode)
    return (
      <>{receiptDrawer}<FrontPosDaily
        online={online}
        loading={!currentTabLoaded}
        data={data}
        tab={tab}
        setTab={setTab}
        busy={busy}
        error={error}
        notice={notice}
        send={send}
        refresh={refresh}
        registerId={registerId}
        setSelected={setSelected}
        setRegisterId={setRegisterId}
        money={money}
        can={can}
        api={api}
        recovery={recovery}
        approvalHref={`${erpBase}/pos?tab=history&queue=approval`}
        erpBase={erpBase}
      /></>
    );
  return (
    <FrontLocalizedTree>
      {receiptDrawer}
      <div className={styles.workspace} data-source="live">
        {recovery}
        <div className={styles.header}>
          <div>
            <span className={styles.muted}>ERPX · {tr("Point of sale")}</span>
            <h1>{tr("Point of sale")}</h1>
            <p className={styles.muted}>
              {tr("Products, accounting and payments connected in one place")}
            </p>
          </div>
          <div className={styles.actions}>
            <Tag value={data.settings?.enabled ? "ready" : "unconfigured"} />
            <Button secondary onClick={() => void refresh().catch((e) => setError(e.message))}>
              Refresh
            </Button>
          </div>
        </div>
        <ModuleTiles
          label={tr("Point of sale")}
          current={tab}
          items={posMenu(language).filter(
            (item) =>
              (
                ({
                  sale: can("pos.sell"),
                  kitchen: can("pos.kitchen"),
                  shifts: can("pos.sell") || can("pos.approve"),
                  history: can("pos.read"),
                  reports: can("pos.reports"),
                  settings: can("pos.configure") || can("pos.payments.configure"),
                }) as Record<string, boolean>
              )[item.id],
          )}
          onSelect={setTab}
        />
        <WidgetGrid>
          {error && (
            <div className={`${styles.alert} ${styles.error}`} role="alert">
              {tr(error)}
            </div>
          )}
          {notice && (
            <div
              className={`${styles.alert} ${styles.success} ${styles.successNotice}`}
              role="status"
              aria-live="polite"
            >
              {tr(notice)}
            </div>
          )}
          {!currentTabLoaded ? (
            <section className={styles.panel} role="status" aria-busy="true">
              <p className={styles.empty}>
                {language === "th" ? "กำลังโหลดข้อมูลจุดขาย…" : "Loading the point of sale…"}
              </p>
            </section>
          ) : (
            <>
              {!data.settings && (
                <div className={styles.alert}>
                  {tr("Configure the account and register before opening a cash session")}{" "}
                  {can("pos.configure") && (
                    <Button secondary onClick={() => setTab("settings")}>
                      POS settings
                    </Button>
                  )}
                </div>
              )}
              {(tab === "settings" || tab === "sale") && (
                <SetupChecklist
                  title={tr("Ready to sell")}
                  items={[
                    {
                      id: "settings",
                      label: tr("Accounting and sales setup"),
                      ready: !!data.settings?.enabled,
                      owner: tr("POS administrator / Finance"),
                      href: `${erpBase}/pos?tab=settings`,
                      detail: data.settings?.enabled
                        ? tr("Configuration enabled")
                        : tr("Complete setup and enable sales"),
                    },
                    {
                      id: "register",
                      label: tr("Active register"),
                      ready: !!register?.active,
                      owner: tr("POS administrator"),
                      href: `${erpBase}/pos?tab=settings`,
                    },
                    {
                      id: "shift",
                      label: tr("Open cash session"),
                      ready: !!shift,
                      owner: tr("Cashier"),
                      href: `${dailyPosRoot}?tab=shifts`,
                      detail: shift
                        ? tr("Ready to sell")
                        : tr("Open a session before the first sale"),
                    },
                    ...(currentTabLoaded
                      ? [
                          {
                            id: "products",
                            label: tr("Products in Commerce"),
                            ready: data.products.length > 0,
                            owner: "Commerce",
                            href: `${erpBase}/commerce`,
                            detail: data.products.length
                              ? `${data.products.length} ${tr("active products available")}`
                              : tr("Add at least one product before the first sale"),
                          },
                        ]
                      : []),
                  ]}
                />
              )}
              {tab === "sale" &&
                currentTabLoaded &&
                data.settings?.enabled &&
                !data.products.length && (
                  <section className={styles.setupCard}>
                    <div>
                      <p className={styles.eyebrow}>Next step</p>
                      <h2>Add a product</h2>
                      <p>
                        POS uses the active products already in Commerce. Add at least one product
                        before opening your first sale.
                      </p>
                    </div>
                    <UiActionLink
                      variant="primary"
                      className={styles.button}
                      href={`${erpBase}/commerce`}
                    >
                      Open Commerce products
                    </UiActionLink>
                  </section>
                )}
              {tab === "sale" && (
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
                                  can("pos.kitchen") && (
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
                                  Verify the room and guest before charging. The hotel folio
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
                                    : "Charged to hotel folio"}{" "}
                                  · Entry {s(data.roomCharge, "folio_entry_id").slice(0, 8)}
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
                                  <p className={styles.muted}>
                                    A different Finance approver can reverse this full room charge.
                                    The hotel folio and restaurant cost records change together;
                                    consumed items become waste unless restocked.
                                  </p>
                                  {can("finance.approve") &&
                                    can("pos.approve") &&
                                    can("pms.approve") && (
                                      <details className={styles.spaced}>
                                        <summary>Reverse room charge</summary>
                                        <form
                                          className={styles.fields}
                                          onSubmit={(e) => {
                                            const f = fields(e);
                                            void saleCommand("sale.reverse_room_charge", {
                                              propertyId: s(data.roomCharge!, "property_id"),
                                              expectedPropertyVersion: n(
                                                data.roomCharge!,
                                                "property_version",
                                              ),
                                              reason: text(f, "reason"),
                                              restockLineIds: f.getAll("restockLine").map(String),
                                            });
                                          }}
                                        >
                                          <Field>
                                            Correction reason
                                            <TextInput name="reason" minLength={3} required />
                                          </Field>
                                          {data.lines
                                            .filter(
                                              (l) =>
                                                l.kind === "stock" &&
                                                l.fulfillment_state === "delivered",
                                            )
                                            .map((l) => (
                                              <Field
                                                orientation="inline"
                                                key={s(l, "id")}
                                                className={styles.check}
                                              >
                                                <TextInput
                                                  type="checkbox"
                                                  name="restockLine"
                                                  value={s(l, "id")}
                                                />
                                                Restock {s(l, "name")} (
                                                {n(l, "quantity_milli") / 1000})
                                              </Field>
                                            ))}
                                          <Button
                                            type="submit"
                                            disabled={busy || data.userId === sale.created_by}
                                          >
                                            Reverse linked room charge
                                          </Button>
                                          {data.userId === sale.created_by && (
                                            <p>
                                              A different Finance approver must review this
                                              correction.
                                            </p>
                                          )}
                                        </form>
                                      </details>
                                    )}
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
              )}
              {tab === "kitchen" && (
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
                            {can("pos.kitchen") && (
                              <Button
                                disabled={busy || !online}
                                onClick={() =>
                                  void send(
                                    `kitchen.${({ draft: "send", queued: "start", preparing: "ready", ready: "deliver" } as Record<string, string>)[s(f, "state")]}`,
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
              )}
              {tab === "history" && (
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
              )}
              {tab === "shifts" && (
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
              )}
              {tab === "reports" && (
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
                          can("pos.approve") &&
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
                          <Link href={`${base}/finance/journals/${st.journal_id}`}>
                            View ledger
                          </Link>
                        )}
                      </div>
                    ))}
                  </FeatureWidget>
                </>
              )}
              {tab === "settings" && (
                <>
                  <UiActionLink
                    variant="primary"
                    className={styles.button}
                    href={`${base}/hospitality/setup`}
                  >
                    Hotel + Restaurant Setup
                  </UiActionLink>
                  <Settings
                    data={data}
                    base={base}
                    busy={busy}
                    send={send}
                    dailyPosRoot={dailyPosRoot}
                    refresh={refresh}
                  />
                  {(can("pos.payments.configure") || can("finance.approve")) && (
                    <UiActionLink
                      variant="primary"
                      className={styles.button}
                      href={`${base}/settings/stripe`}
                    >
                      Company Stripe · POS and PMS payments
                    </UiActionLink>
                  )}
                </>
              )}
              {data.jobs.some((j) => j.state === "exception") && (
                <section role="alert" className={styles.panel}>
                  <h2>Items requiring review</h2>
                  {data.jobs
                    .filter((j) => j.state === "exception")
                    .map((j) => (
                      <div className={styles.row} key={s(j, "id")}>
                        <div>
                          <strong>{s(j, "kind")}</strong>
                          <span>{s(j, "last_error")}</span>
                          {j.last_error === "POS_PROVIDER_REFUND_EMAIL_RESTRICTED" && (
                            <p>
                              Stripe cannot send PromptPay refund instructions. Verify the merchant
                              email or complete the account application in Stripe, then retry the
                              original refund.
                            </p>
                          )}
                        </div>
                        {can("pos.approve") && (
                          <form
                            className={styles.form}
                            onSubmit={(e) => {
                              const f = fields(e),
                                provider = text(f, "provider"),
                                email = text(f, "instructionsEmail");
                              if (provider && email) {
                                setError(
                                  "Attach the existing Stripe refund or correct its instruction email separately.",
                                );
                                return;
                              }
                              void send(
                                provider ? "exception.attach_provider" : "exception.retry",
                                {
                                  id: j.id,
                                  accountingDate: text(f, "date"),
                                  reason: text(f, "reason"),
                                  ...(provider ? { providerId: provider } : {}),
                                  ...(email
                                    ? {
                                        refundInstructionsEmail: email,
                                        expectedRefundVersion: n(j, "refund_version"),
                                      }
                                    : {}),
                                },
                              );
                            }}
                          >
                            <Field>
                              Accounting date
                              <TextInput name="date" type="date" required />
                            </Field>
                            {["refund", "checkout"].includes(s(j, "kind")) && (
                              <Field>
                                Detected Stripe ID (if any)
                                <TextInput name="provider" placeholder="re_… / pyr_… / cs_…" />
                              </Field>
                            )}
                            {j.kind === "refund" &&
                              j.last_error === "POS_PROVIDER_REFUND_EMAIL_RESTRICTED" &&
                              can("finance.approve") && (
                                <Field>
                                  Refund instructions email (optional)
                                  <TextInput
                                    name="instructionsEmail"
                                    type="email"
                                    maxLength={254}
                                    autoComplete="off"
                                  />
                                  <small>
                                    Use an email supplied by the customer and permitted by Stripe.
                                    Leave blank to retry the original recipient. CRM and receipt
                                    details stay unchanged.
                                  </small>
                                </Field>
                              )}
                            <Field>
                              Correction reason
                              <TextInput name="reason" minLength={3} required />
                            </Field>
                            <Button type="submit" disabled={busy || !online}>
                              Retry with the same item
                            </Button>
                          </form>
                        )}
                      </div>
                    ))}
                </section>
              )}
              {printing && <PosReceipt receipt={data.receipt} />}
              {tab === "sale" &&
                sale &&
                ["open", "held"].includes(s(sale, "state")) &&
                can("pos.sell") && (
                  <FeatureWidget
                    id="pos-void-evidence"
                    title="Cancel sale with evidence"
                    className={styles.panel}
                  >
                    <form
                      className={styles.form}
                      onSubmit={(event) => {
                        const form = fields(event),
                          setId = text(form, "attachmentSetId");
                        void saleCommand("sale.cancel", { attachmentSetIds: setId ? [setId] : [] });
                      }}
                    >
                      <AttachmentPanel
                        base={base}
                        targetType="pos_sale"
                        targetId={s(sale, "id")}
                        expectedTargetVersion={n(sale, "version")}
                        componentKind="action"
                        componentRef="void"
                        purposeKey="void_evidence"
                        title="Void evidence"
                        description="Optionally attach the supervisor note, bill image, or other evidence supporting this cancellation."
                        visibility="Visible to authorized POS colleagues in this Branch."
                        compact
                        fieldName="attachmentSetId"
                      />
                      <Button type="submit" secondary disabled={busy || !online}>
                        Cancel this sale
                      </Button>
                    </form>
                  </FeatureWidget>
                )}
              {tab === "shifts" &&
                can("pos.approve") &&
                data.shifts
                  .filter((item) => s(item, "state") === "closed")
                  .map((item) => (
                    <FeatureWidget
                      key={`variance-${s(item, "id")}`}
                      id={`pos-cash-variance-${s(item, "id")}`}
                      title={`Cash variance · ${s(item, "register_name")}`}
                      className={styles.panel}
                    >
                      {item.opened_by === data.userId || item.closed_by === data.userId ? (
                        <p role="status" className={styles.muted}>
                          {language === "th"
                            ? "ผู้เปิดหรือปิดกะต้องให้ผู้จัดการอีกคนตรวจยอดเงินสด"
                            : "A different manager must review this cash session because you opened or closed it."}
                        </p>
                      ) : (
                        <form
                          className={styles.form}
                          onSubmit={(event) => {
                            const form = fields(event),
                              setId = text(form, "attachmentSetId");
                            void send("shift.review", {
                              id: item.id,
                              expectedVersion: item.version,
                              reason: text(form, "reason"),
                              attachmentSetIds: setId ? [setId] : [],
                            });
                          }}
                        >
                          <Field>
                            Review result
                            <TextInput name="reason" required minLength={3} />
                          </Field>
                          <AttachmentPanel
                            base={base}
                            targetType="pos_shift"
                            targetId={s(item, "id")}
                            expectedTargetVersion={n(item, "version")}
                            componentKind="action"
                            componentRef="variance-review"
                            purposeKey="cash_variance"
                            title="Cash variance evidence"
                            description="Attach the count sheet, till photo, or investigation support reviewed for this variance."
                            visibility="Visible to authorized POS reviewers in this Branch."
                            compact
                            fieldName="attachmentSetId"
                          />
                          <Button type="submit" disabled={busy || !online}>
                            Review and record variance
                          </Button>
                        </form>
                      )}
                    </FeatureWidget>
                  ))}
            </>
          )}
        </WidgetGrid>
      </div>
    </FrontLocalizedTree>
  );
}
type Send = (
  op: string,
  input: unknown,
  select?: boolean,
) => Promise<Record<string, unknown> | undefined>;
function CatalogImageUpload({
  product,
  api,
  refresh,
}: {
  product: Row;
  api: string;
  refresh: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [retry, setRetry] = useState(false);
  const pending = useRef<{ file: File; key: string; version: number } | null>(null);
  async function upload(e: FormEvent<HTMLFormElement>) {
    const f = fields(e),
      file = pending.current?.file ?? f.get("image");
    if (!(file instanceof File) || !file.size) return;
    if (file.size > 10 * 1024 * 1024) {
      setError("ใช้ไฟล์ PNG, JPEG หรือ WebP ไม่เกิน 10 MB");
      return;
    }
    const command = pending.current ?? {
      file,
      key: crypto.randomUUID(),
      version: n(product, "catalog_version"),
    };
    pending.current = command;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const r = await workFetch(`${api}/images/upload`, {
        method: "POST",
        headers: {
          "Content-Type": command.file.type,
          "Idempotency-Key": command.key,
          "x-pos-product": s(product, "id"),
          "x-pos-catalog-version": String(command.version),
        },
        body: command.file,
      });
      const body = await r.json();
      if (!r.ok) {
        if (r.status < 500) pending.current = null;
        throw new Error(body.error?.message ?? "Upload failed");
      }
      pending.current = null;
      setNotice("บันทึกรูปสินค้าและ thumbnail แล้ว");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Upload failed");
    } finally {
      setBusy(false);
      setRetry(!!pending.current);
    }
  }
  return (
    <form className={styles.form} onSubmit={(e) => void upload(e)}>
      <Field>
        รูปสินค้า / Product image
        <TextInput
          name="image"
          type="file"
          accept="image/png,image/jpeg,image/webp"
          required={!retry}
          disabled={busy || retry}
        />
      </Field>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <Button type="submit" disabled={busy}>
        {retry ? "ตรวจผล upload เดิม / Retry upload" : "บันทึกรูปสินค้า / Save image"}
      </Button>
    </form>
  );
}
function Ingredients({
  products,
  value,
  onChange,
}: {
  products: Row[];
  value: Array<{
    productId: string;
    quantityMilli: number;
    unitLabel?: string;
    conversionMicros?: number;
  }>;
  onChange: (
    v: Array<{
      productId: string;
      quantityMilli: number;
      unitLabel?: string;
      conversionMicros?: number;
    }>,
  ) => void;
}) {
  return (
    <div className={styles.ingredients}>
      {value.map((v, i) => (
        <div key={i}>
          <Select
            aria-label={`Ingredient ${i + 1}`}
            value={v.productId}
            onChange={(e) =>
              onChange(value.map((p, j) => (j === i ? { ...p, productId: e.target.value } : p)))
            }
          >
            <option value="">Select ingredient</option>
            {products.map((p) => (
              <option key={s(p, "id")} value={s(p, "id")}>
                {s(p, "name")} ({s(p, "unit")})
              </option>
            ))}
          </Select>
          <TextInput
            aria-label={`Ingredient quantity ${i + 1}`}
            type="number"
            min="0.001"
            step="0.001"
            value={v.quantityMilli / 1000}
            onChange={(e) =>
              onChange(
                value.map((p, j) =>
                  j === i ? { ...p, quantityMilli: Math.round(Number(e.target.value) * 1000) } : p,
                ),
              )
            }
          />
          <TextInput
            aria-label={`Ingredient unit ${i + 1}`}
            placeholder="Entered unit"
            value={v.unitLabel ?? ""}
            onChange={(e) =>
              onChange(value.map((p, j) => (j === i ? { ...p, unitLabel: e.target.value } : p)))
            }
          />
          <Field>
            1 unit = base unit
            <TextInput
              aria-label={`Conversion rate ${i + 1}`}
              type="number"
              min="0.000001"
              step="0.000001"
              value={(v.conversionMicros ?? 1000000) / 1000000}
              onChange={(e) =>
                onChange(
                  value.map((p, j) =>
                    j === i
                      ? { ...p, conversionMicros: Math.round(Number(e.target.value) * 1000000) }
                      : p,
                  ),
                )
              }
            />
          </Field>
          <Button secondary onClick={() => onChange(value.filter((_, j) => i !== j))}>
            Remove
          </Button>
        </div>
      ))}
      <Button
        secondary
        onClick={() => onChange([...value, { productId: "", quantityMilli: 1000 }])}
      >
        Add ingredient
      </Button>
    </div>
  );
}
function Catalog({
  data,
  busy,
  send,
  allowRecipes,
  api,
  refresh,
}: {
  data: PosWorkspace;
  busy: boolean;
  send: Send;
  allowRecipes: boolean;
  api: string;
  refresh: () => Promise<void>;
}) {
  const [productId, setProductId] = useState(""),
    [catalogKind, setCatalogKind] = useState("stock"),
    [recipe, setRecipe] = useState<
      Array<{
        productId: string;
        quantityMilli: number;
        unitLabel?: string;
        conversionMicros?: number;
      }>
    >([]),
    [modifiers, setModifiers] = useState<
      Array<{
        id: string;
        name: string;
        priceMinor: number;
        ingredients: Array<{
          productId: string;
          quantityMilli: number;
          unitLabel?: string;
          conversionMicros?: number;
        }>;
      }>
    >([]);
  const product = data.products.find((p) => p.id === productId);
  return (
    <FeatureWidget
      id="pos-products-menus-recipes"
      title={allowRecipes ? "Products, menus & recipes" : "Product scan details"}
      className={styles.panel}
    >
      <div className={styles.fields}>
        <Field>
          Product
          <Select
            value={productId}
            onChange={(e) => {
              const p = data.products.find((p) => p.id === e.target.value);
              setProductId(e.target.value);
              setCatalogKind(s(p ?? {}, "kind") || "stock");
              setRecipe(JSON.parse(s(p ?? {}, "recipe_json") || "[]"));
              setModifiers(JSON.parse(s(p ?? {}, "modifiers_json") || "[]"));
            }}
          >
            <option value="">Select product</option>
            {data.products.map((p) => (
              <option key={s(p, "id")} value={s(p, "id")}>
                {s(p, "name")}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      {product?.kind === "recipe" && !allowRecipes ? (
        <p className={styles.muted}>
          Recipe details are available after a restaurant register is added.
        </p>
      ) : (
        product && (
          <form
            key={`${productId}:${product.catalog_version}`}
            className={`${styles.fields} ${styles.spaced}`}
            onSubmit={(e) => {
              const f = fields(e);
              void send("catalog.save", {
                productId,
                barcode: text(f, "barcode"),
                category: text(f, "category"),
                kind: text(f, "kind"),
                station: allowRecipes ? text(f, "station") : "Kitchen",
                taxInclusive: f.get("taxInclusive") === "on",
                enabled: f.get("enabled") === "on",
                registerIds: f.getAll("register").map(String),
                recipe,
                modifiers,
                expectedVersion: product.catalog_version,
              });
            }}
          >
            <Field orientation="inline" className={styles.check}>
              <TextInput name="enabled" type="checkbox" defaultChecked={Boolean(product.enabled)} />
              เปิดขายใน POS / Enabled for sale
            </Field>
            <fieldset>
              <legend>จุดขายที่เปิดขาย / Register catalog</legend>
              {data.registers.map((r) => (
                <Field orientation="inline" key={s(r, "id")} className={styles.check}>
                  <TextInput
                    type="checkbox"
                    name="register"
                    value={s(r, "id")}
                    defaultChecked={JSON.parse(s(product, "register_ids_json") || "[]").includes(
                      r.id,
                    )}
                  />
                  {s(r, "name")}
                </Field>
              ))}
            </fieldset>
            <Field>
              Barcode
              <TextInput name="barcode" defaultValue={s(product, "barcode")} />
            </Field>
            <Field>
              Category
              <TextInput name="category" defaultValue={s(product, "category")} />
            </Field>
            <Field>
              Type
              <Select
                name="kind"
                value={catalogKind}
                onChange={(event) => setCatalogKind(event.target.value)}
              >
                <option value="stock">Stock item</option>
                {allowRecipes && <option value="recipe">Recipe item</option>}
                <option value="service">Service</option>
              </Select>
            </Field>
            {allowRecipes && catalogKind === "recipe" ? (
              <Field>
                Kitchen station
                <TextInput name="station" defaultValue={s(product, "station")} required />
              </Field>
            ) : (
              <TextInput type="hidden" name="station" value={s(product, "station") || "Kitchen"} />
            )}
            <Field orientation="inline" className={styles.check}>
              <TextInput
                name="taxInclusive"
                type="checkbox"
                defaultChecked={Boolean(product.tax_inclusive)}
              />
              Product price includes tax
            </Field>
            {allowRecipes && (
              <>
                {catalogKind === "recipe" && (
                  <div>
                    <strong>Ingredients per menu unit</strong>
                    <p className={styles.muted}>Base-unit quantity, e.g. 0.2 kg = 200 g</p>
                    <Ingredients products={data.products} value={recipe} onChange={setRecipe} />
                  </div>
                )}
                <div>
                  <strong>{catalogKind === "recipe" ? "Menu modifiers" : "Product options"}</strong>
                  {modifiers.map((m, i) => (
                    <div key={m.id} className={styles.panel}>
                      <Field>
                        Name
                        <TextInput
                          value={m.name}
                          onChange={(e) =>
                            setModifiers(
                              modifiers.map((v, j) =>
                                j === i ? { ...v, name: e.target.value } : v,
                              ),
                            )
                          }
                        />
                      </Field>
                      <Field>
                        Additional price
                        <TextInput
                          type="number"
                          min="0"
                          step="0.01"
                          value={m.priceMinor / 100}
                          onChange={(e) =>
                            setModifiers(
                              modifiers.map((v, j) =>
                                j === i
                                  ? { ...v, priceMinor: Math.round(Number(e.target.value) * 100) }
                                  : v,
                              ),
                            )
                          }
                        />
                      </Field>
                      {(catalogKind === "recipe" || m.ingredients.length > 0) && (
                        <>
                          {catalogKind !== "recipe" && (
                            <p className={styles.muted}>
                              Ingredient options require a recipe item. Remove ingredients or choose
                              Recipe item before saving.
                            </p>
                          )}
                          <Ingredients
                            products={data.products}
                            value={m.ingredients}
                            onChange={(ingredients) =>
                              setModifiers(
                                modifiers.map((v, j) => (j === i ? { ...v, ingredients } : v)),
                              )
                            }
                          />
                        </>
                      )}
                      <Button
                        secondary
                        onClick={() => setModifiers(modifiers.filter((_, j) => j !== i))}
                      >
                        Remove modifier
                      </Button>
                    </div>
                  ))}
                  <Button
                    secondary
                    onClick={() =>
                      setModifiers([
                        ...modifiers,
                        { id: crypto.randomUUID(), name: "", priceMinor: 0, ingredients: [] },
                      ])
                    }
                  >
                    Add modifier
                  </Button>
                </div>
              </>
            )}
            <Button type="submit" disabled={busy}>
              Save POS product
            </Button>
          </form>
        )
      )}
      {product && n(product, "catalog_version") > 0 && (
        <CatalogImageUpload key={productId} product={product} api={api} refresh={refresh} />
      )}
    </FeatureWidget>
  );
}
function RetailSetup({
  data,
  base,
  busy,
  send,
}: {
  data: PosWorkspace;
  base: string;
  busy: boolean;
  send: Send;
}) {
  const [mode, setMode] = useState<"retail" | "restaurant">("retail"),
    [registerName, setRegisterName] = useState("");
  const activeRetailRegisters = data.registers.filter(
    (row) => s(row, "mode") === mode && Boolean(row.active),
  );
  const existingRetailRegisters = data.registers.filter((row) => s(row, "mode") === mode);
  const [registerId, setRegisterId] = useState(
      activeRetailRegisters.length === 1 ? s(activeRetailRegisters[0], "id") : "",
    ),
    [warehouseId, setWarehouseId] = useState(""),
    [bankAccountId, setBankAccountId] = useState("");
  const commerce = data.commerce,
    chartId = s(commerce ?? {}, "chart_id"),
    currency = s(commerce ?? {}, "currency");
  const chart = data.charts.find((row) => s(row, "id") === chartId);
  const warehouses = data.warehouses;
  const selectedRegister = activeRetailRegisters.find((row) => s(row, "id") === registerId);
  const selectedWarehouse = selectedRegister
    ? warehouses.find((row) => s(row, "id") === s(selectedRegister, "warehouse_id"))
    : (warehouses.find((row) => s(row, "id") === warehouseId) ??
      (warehouses.length === 1 ? warehouses[0] : undefined));
  const bankAccounts = data.bankAccounts.filter(
    (row) =>
      s(row, "chart_id") === chartId && (!s(row, "currency") || s(row, "currency") === currency),
  );
  const commerceAccounts = posCommerceAccountDefaults.map((source) => {
    const accountId = s(commerce ?? {}, source.field);
    const account = data.accounts.find(
      (row) =>
        s(row, "id") === accountId &&
        s(row, "chart_id") === chartId &&
        s(row, "category") === source.category &&
        (!s(row, "currency") || s(row, "currency") === currency),
    );
    return { source, account };
  });
  const issues: string[] = [];
  if (!commerce) issues.push("Set up Commerce and Finance before creating POS settings.");
  else {
    if (!chart || s(chart, "status") !== "active")
      issues.push("Select an active chart of accounts in Commerce and Finance.");
    if (chart && s(chart, "base_currency") !== currency)
      issues.push(
        `Commerce uses ${currency}, but the chart base currency is ${s(chart, "base_currency")}. Align them in Finance before continuing.`,
      );
    if (commerceAccounts.some((item) => !item.account))
      issues.push(
        "One or more Commerce accounts are unavailable or use a different currency. Review Commerce and Finance settings.",
      );
    if (!activeRetailRegisters.length && existingRetailRegisters.length)
      issues.push(
        "The existing retail register is inactive. Add an active retail register in Advanced POS settings before setup; the inactive register will remain unchanged.",
      );
    if (activeRetailRegisters.length > 1 && !selectedRegister)
      issues.push("Choose which active retail register will use the POS settings.");
    if (selectedRegister && !selectedWarehouse)
      issues.push("The chosen retail register must use an active warehouse in its branch.");
    if (!activeRetailRegisters.length && !existingRetailRegisters.length && !warehouses.length)
      issues.push("Add an active warehouse in Commerce before creating a register.");
    if (
      !activeRetailRegisters.length &&
      !existingRetailRegisters.length &&
      warehouses.length > 1 &&
      !warehouses.some((row) => s(row, "id") === warehouseId)
    )
      issues.push("Choose which active warehouse the retail register will use.");
    if (bankAccounts.length > 1 && !bankAccounts.some((row) => s(row, "id") === bankAccountId))
      issues.push("Choose which bank account will receive POS transfer payments.");
  }
  const canBootstrap = Boolean(
    data.permissions["pos.configure"] && data.permissions["finance.write"],
  );
  const chosenWarehouse = selectedWarehouse;
  const chosenBank =
    bankAccounts.find((row) => s(row, "id") === bankAccountId) ??
    (bankAccounts.length === 1 ? bankAccounts[0] : undefined);
  const generatedAccounts = posGeneratedAccountSpecs.filter(
    (spec) => spec.key !== "bank" || bankAccounts.length === 0,
  );
  const submit = () =>
    void send("settings.bootstrap", {
      mode,
      ...(registerName.trim() ? { registerName: registerName.trim() } : {}),
      ...(selectedRegister ? { registerId: s(selectedRegister, "id") } : {}),
      ...(!selectedRegister && chosenWarehouse ? { warehouseId: s(chosenWarehouse, "id") } : {}),
      ...(chosenBank ? { bankAccountId: s(chosenBank, "id") } : {}),
    });

  return (
    <section className={styles.setupCard} aria-labelledby="pos-retail-setup-title">
      <div className={styles.setupHeading}>
        <p className={styles.eyebrow}>
          First-time setup · {mode === "restaurant" ? "Restaurant" : "Retail"}
        </p>
        <h2 id="pos-retail-setup-title">
          Set up POS for {mode === "restaurant" ? "a restaurant" : "a retail store"}
        </h2>
        <p>
          POS will reuse your Commerce accounts and add the accounts listed below. You can open a
          cash session after setup.
        </p>
      </div>
      <Field className={styles.setupSelect}>
        Business mode
        <Select
          value={mode}
          onChange={(e) => {
            setMode(e.target.value as "retail" | "restaurant");
            setRegisterId("");
          }}
        >
          <option value="retail">Retail</option>
          <option value="restaurant">Restaurant</option>
        </Select>
      </Field>
      <Field className={styles.setupSelect}>
        New register name
        <TextInput
          value={registerName}
          placeholder={`Main ${mode} register`}
          onChange={(e) => setRegisterName(e.target.value)}
          maxLength={200}
        />
      </Field>
      {!commerce ? (
        <div className={styles.blocker}>
          <strong>Commerce setup is required first.</strong>
          <p>
            POS uses the company chart, currency, accounts, and active warehouse configured in
            Commerce and Finance.
          </p>
          <div className={styles.actions}>
            <UiActionLink variant="primary" className={styles.button} href={`${base}/commerce`}>
              Open Commerce setup
            </UiActionLink>
            <UiActionLink
              variant="secondary"
              className={`${styles.button} ${styles.secondary}`}
              href={`${base}/finance/setup`}
            >
              Open Finance setup
            </UiActionLink>
          </div>
        </div>
      ) : (
        <>
          <dl className={styles.setupSummary}>
            <div>
              <dt>Chart of accounts</dt>
              <dd>
                {s(chart ?? {}, "name") || "Not available"}
                {chart && <> · {s(chart, "base_currency")}</>}
              </dd>
            </div>
            <div>
              <dt>Commerce currency</dt>
              <dd>{currency || "Not configured"}</dd>
            </div>
            <div>
              <dt>{selectedRegister ? "Existing retail register" : "Register warehouse"}</dt>
              <dd>
                {selectedRegister
                  ? `${s(selectedRegister, "name")} · ${s(chosenWarehouse ?? {}, "name") || "Warehouse unavailable"}`
                  : chosenWarehouse
                    ? s(chosenWarehouse, "name")
                    : "Choose an active warehouse"}
              </dd>
            </div>
            <div>
              <dt>POS transfer bank</dt>
              <dd>
                {chosenBank
                  ? `${s(chosenBank, "code")} · ${s(chosenBank, "name")}`
                  : bankAccounts.length
                    ? "Choose a bank account"
                    : "A POS bank account will be created"}
              </dd>
            </div>
          </dl>
          {activeRetailRegisters.length > 1 && (
            <Field className={styles.setupSelect}>
              Retail register to configure
              <Select value={registerId} onChange={(event) => setRegisterId(event.target.value)}>
                <option value="">Choose a retail register</option>
                {activeRetailRegisters.map((row) => (
                  <option key={s(row, "id")} value={s(row, "id")}>
                    {s(row, "name")} · {s(row, "warehouse_name")}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          {!activeRetailRegisters.length &&
            !existingRetailRegisters.length &&
            warehouses.length > 1 && (
              <Field className={styles.setupSelect}>
                Warehouse for the retail register
                <Select
                  value={warehouseId}
                  onChange={(event) => setWarehouseId(event.target.value)}
                >
                  <option value="">Choose a warehouse</option>
                  {warehouses.map((row) => (
                    <option key={s(row, "id")} value={s(row, "id")}>
                      {s(row, "name")}
                    </option>
                  ))}
                </Select>
              </Field>
            )}
          {bankAccounts.length > 1 && (
            <Field className={styles.setupSelect}>
              Bank account for transfer receipts
              <Select
                value={bankAccountId}
                onChange={(event) => setBankAccountId(event.target.value)}
              >
                <option value="">Choose a bank account</option>
                {bankAccounts.map((row) => (
                  <option key={s(row, "id")} value={s(row, "id")}>
                    {s(row, "code")} · {s(row, "name")}
                    {s(row, "currency") ? ` · ${s(row, "currency")}` : ""}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          <div className={styles.accountPlan}>
            <section className={styles.accountGroup} aria-labelledby="pos-commerce-accounts-title">
              <div className={styles.accountGroupHeading}>
                <div>
                  <p className={styles.planKicker}>Reuse from Commerce</p>
                  <h3 id="pos-commerce-accounts-title">Existing accounts</h3>
                </div>
                <span className={styles.planCount}>{commerceAccounts.length} accounts</span>
              </div>
              <ul>
                {commerceAccounts.map(({ source, account }) => (
                  <li key={source.key}>
                    <span>{source.label}</span>
                    <strong>
                      {account ? `${s(account, "code")} · ${s(account, "name")}` : "Unavailable"}
                    </strong>
                  </li>
                ))}
              </ul>
            </section>
            <section className={styles.accountGroup} aria-labelledby="pos-new-accounts-title">
              <div className={styles.accountGroupHeading}>
                <div>
                  <p className={styles.planKicker}>Add to this chart</p>
                  <h3 id="pos-new-accounts-title">POS accounts</h3>
                </div>
                <span className={styles.planCount}>{generatedAccounts.length} accounts</span>
              </div>
              <p className={styles.planNote}>
                If a code is already in use, setup adds a numeric suffix and leaves that account
                unchanged.
              </p>
              <ul>
                {generatedAccounts.map((spec) => (
                  <li key={spec.key}>
                    <span>{spec.name}</span>
                    <strong>
                      {spec.code} · {spec.category}
                      {spec.key === "bank" ? " · bank control" : ""}
                    </strong>
                  </li>
                ))}
              </ul>
            </section>
          </div>
        </>
      )}
      {issues.length > 0 && (
        <div className={styles.blocker} role="alert">
          <strong>Resolve these items before setup</strong>
          <ul>
            {issues.map((issue) => (
              <li key={issue}>{issue}</li>
            ))}
          </ul>
          {commerce && (
            <div className={styles.actions}>
              <UiActionLink
                variant="secondary"
                className={`${styles.button} ${styles.secondary}`}
                href={`${base}/commerce`}
              >
                Review Commerce
              </UiActionLink>
              <UiActionLink
                variant="secondary"
                className={`${styles.button} ${styles.secondary}`}
                href={`${base}/finance/setup`}
              >
                Review Finance
              </UiActionLink>
            </div>
          )}
        </div>
      )}
      {!canBootstrap && (
        <p className={styles.muted}>
          A user with both POS configure and Finance write access must run this setup.
        </p>
      )}
      <div className={styles.setupActions}>
        <Button onClick={submit} disabled={busy || issues.length > 0 || !canBootstrap}>
          Set up POS for {mode}
        </Button>
        <span className={styles.muted}>
          {selectedRegister
            ? "Adds POS settings to the existing register without changing it."
            : "Creates POS settings and one register in a single save."}{" "}
          It does not open a cash session.
        </span>
      </div>
    </section>
  );
}
function Settings({
  data,
  base,
  busy,
  send,
  dailyPosRoot,
  refresh,
}: {
  data: PosWorkspace;
  base: string;
  busy: boolean;
  send: Send;
  dailyPosRoot: string;
  refresh: () => Promise<void>;
}) {
  const fallback = data.commerce ?? {},
    stored = data.settings,
    accounts = stored ? (JSON.parse(s(stored, "accounts_json")) as PosAccounts) : null;
  const savedChartId = accounts?.chartId ?? s(fallback, "chart_id");
  const initialMappings = Object.fromEntries(
    accountKeys.map((key) => [key, accounts?.[key] ?? ""]),
  );
  const [chartId, setChartId] = useState(savedChartId);
  const [mappingValues, setMappingValues] = useState<Record<string, string>>(initialMappings);
  const [accountingOpen, setAccountingOpen] = useState(false),
    [showMappingErrors, setShowMappingErrors] = useState(false);
  const [enabled, setEnabled] = useState(Boolean(stored?.enabled)),
    [discount, setDiscount] = useState(n(stored ?? {}, "cashier_discount_bps") / 100);
  const accountNames: Record<string, string> = {
    cash: "Cash",
    bank: "Bank deposits",
    edc: "EDC clearing",
    stripe: "Stripe clearing",
    receivable: "POS receivables",
    revenue: "Revenue",
    outputTax: "Output tax",
    inventory: "Inventory",
    cogs: "Cost of sales",
    advance: "Advances received",
    wip: "Work in progress",
    waste: "Waste",
    refund: "Customer refunds payable",
    fees: "Fees and losses",
    cashDifference: "Cash variance",
    cashTransfer: "Cash drawer transfers",
    disputes: "Disputes pending",
  };
  const mappingGroups: { title: string; keys: (typeof accountKeys)[number][] }[] = [
    { title: "Payments and clearing", keys: ["cash", "bank", "edc", "stripe", "receivable"] },
    { title: "Sales and liabilities", keys: ["revenue", "outputTax", "advance", "disputes"] },
    { title: "Inventory and fulfilment", keys: ["inventory", "cogs", "wip", "waste"] },
    {
      title: "Refunds and adjustments",
      keys: ["refund", "fees", "cashDifference", "cashTransfer"],
    },
  ];
  const chart = data.charts.find((row) => s(row, "id") === chartId),
    chartChanged = Boolean(chartId && chartId !== savedChartId);
  const isMappingValid = (key: string, value = mappingValues[key] ?? "") =>
    Boolean(
      chartId &&
      value &&
      data.accounts.some((row) => s(row, "id") === value && s(row, "chart_id") === chartId),
    );
  const missingMappings = accountKeys.filter((key) => !isMappingValid(key));
  const activeRegisters = data.registers.filter((row) => Boolean(row.active));
  const barcodedProducts = data.products.filter((row) => s(row, "barcode").trim()).length;
  const accountingSummary = chart
    ? `${s(chart, "name")} · ${accountKeys.length - missingMappings.length}/${accountKeys.length} mappings ready`
    : "Select a chart of accounts";
  const registerSummary = data.registers.length
    ? `${activeRegisters.length} active · ${data.registers.length} total`
    : "No registers configured";
  const productSummary = data.products.length
    ? `${data.products.length} products · ${barcodedProducts} with barcode`
    : "No active products";
  const canConfigure = Boolean(data.permissions["pos.configure"]);
  const handleChartChange = (nextChartId: string) => {
    setChartId(nextChartId);
    setMappingValues(
      nextChartId === savedChartId
        ? initialMappings
        : Object.fromEntries(accountKeys.map((key) => [key, ""])),
    );
    setShowMappingErrors(false);
    if (nextChartId !== savedChartId) setAccountingOpen(true);
  };
  const saveSettings = (event: FormEvent<HTMLFormElement>) => {
    const f = fields(event),
      nextChartId = text(f, "chartId");
    const nextMappings = Object.fromEntries(accountKeys.map((key) => [key, text(f, key)]));
    const missing = accountKeys.filter(
      (key) =>
        !nextChartId ||
        !text(f, key) ||
        !data.accounts.some(
          (row) => s(row, "id") === text(f, key) && s(row, "chart_id") === nextChartId,
        ),
    );
    if (missing.length) {
      setShowMappingErrors(true);
      setAccountingOpen(true);
      return;
    }
    void send("settings.save", {
      enabled: f.get("enabled") === "on",
      currency: text(f, "currency"),
      timezone: "Asia/Bangkok",
      cashierDiscountBps: Math.round(Number(f.get("discount")) * 100),
      expectedVersion: stored?.version ?? 0,
      accounts: { ...nextMappings, chartId: nextChartId },
    });
  };
  const setupContent = stored ? (
    <section className={styles.setupCard} aria-labelledby="pos-setup-status">
      <div>
        <p className={styles.eyebrow}>POS setup</p>
        <h2 id="pos-setup-status">{stored.enabled ? "POS is ready" : "POS is paused"}</h2>
        <p>
          {stored.enabled
            ? "Registers and accounting policy are configured. Open a cash session with the opening float when the location is ready to trade."
            : "POS settings are saved, but new sales are paused."}
        </p>
      </div>
      <div className={styles.actions}>
        <UiActionLink
          variant="primary"
          className={styles.button}
          href={`${dailyPosRoot}?tab=shifts`}
        >
          Open a cash session
        </UiActionLink>
        <UiActionLink
          variant="secondary"
          className={`${styles.button} ${styles.secondary}`}
          href={`${dailyPosRoot}?tab=sale`}
        >
          Go to sales
        </UiActionLink>
      </div>
      {data.products.length === 0 && (
        <p className={styles.notice}>
          Add products in <Link href={`${base}/commerce`}>Commerce</Link> before the first sale.
        </p>
      )}
    </section>
  ) : canConfigure ? (
    <RetailSetup data={data} base={base} busy={busy} send={send} />
  ) : (
    <section className={styles.setupCard}>
      <div>
        <p className={styles.eyebrow}>POS setup</p>
        <h2>POS is not configured</h2>
        <p>A POS administrator must complete the first-time setup.</p>
      </div>
    </section>
  );
  if (!canConfigure) return setupContent;
  return (
    <>
      {setupContent}
      <FeatureWidget id="pos-sales-settings" title="Sales settings" className={styles.panel}>
        <form className={styles.settingsForm} onSubmit={saveSettings}>
          <div className={styles.salesSettings}>
            <Field orientation="inline" className={styles.salesToggle}>
              <TextInput
                name="enabled"
                type="checkbox"
                checked={enabled}
                onChange={(event) => setEnabled(event.target.checked)}
              />
              <span>
                <strong>Accept new POS sales</strong>
                <small>Pause POS sales without changing the register or accounting setup.</small>
              </span>
            </Field>
            <Field>
              Maximum cashier discount (%)
              <TextInput
                name="discount"
                type="number"
                min="0"
                max="100"
                step="0.01"
                value={discount}
                onChange={(event) => setDiscount(Number(event.target.value))}
              />
            </Field>
          </div>
          <details
            className={styles.settingsDisclosure}
            open={accountingOpen || chartChanged}
            onToggle={(event) => {
              if (!chartChanged) setAccountingOpen(event.currentTarget.open);
            }}
          >
            <summary>
              <span>Accounting</span>
              <span className={styles.disclosureSummaryValue}>{accountingSummary}</span>
            </summary>
            <div className={styles.disclosureBody}>
              <p className={styles.muted}>
                These mappings connect POS payments, sales, inventory and adjustments to the company
                chart.
              </p>
              {chartChanged && (
                <p className={styles.mappingNotice} role="status">
                  Chart changed. Select an account from this chart for each mapping before saving.
                </p>
              )}
              {showMappingErrors && missingMappings.length > 0 && (
                <p className={styles.mappingErrorSummary} role="alert">
                  Select accounts for: {missingMappings.map((key) => accountNames[key]).join(", ")}.
                </p>
              )}
              <div className={styles.accountingControls}>
                <Field>
                  Currency
                  <TextInput
                    name="currency"
                    defaultValue={s(stored ?? fallback, "currency") || "THB"}
                    pattern="[A-Z]{3}"
                    required
                  />
                </Field>
                <Field>
                  Chart of accounts
                  <Select
                    name="chartId"
                    value={chartId}
                    onChange={(event) => handleChartChange(event.target.value)}
                  >
                    <option value="">Select chart of accounts</option>
                    {data.charts.map((row) => (
                      <option key={s(row, "id")} value={s(row, "id")}>
                        {s(row, "name")} · {s(row, "base_currency")}
                      </option>
                    ))}
                  </Select>
                </Field>
              </div>
              {mappingGroups.map((group) => (
                <section
                  className={styles.accountMappingGroup}
                  key={group.title}
                  aria-label={group.title}
                >
                  <div className={styles.mappingGroupHeading}>
                    <h3>{group.title}</h3>
                    <span>{group.keys.length} mappings</span>
                  </div>
                  <div className={styles.accountMappingGrid}>
                    {group.keys.map((key) => {
                      const invalid = (showMappingErrors || chartChanged) && !isMappingValid(key);
                      const helpId = `pos-account-${key}-help`;
                      return (
                        <Field key={key} className={styles.mappingField}>
                          {accountNames[key]}
                          <Select
                            name={key}
                            value={mappingValues[key] ?? ""}
                            onChange={(event) =>
                              setMappingValues((current) => ({
                                ...current,
                                [key]: event.target.value,
                              }))
                            }
                            aria-invalid={invalid}
                            aria-describedby={helpId}
                          >
                            <option value="">Select account</option>
                            {data.accounts
                              .filter((row) => s(row, "chart_id") === chartId)
                              .map((row) => (
                                <option key={s(row, "id")} value={s(row, "id")}>
                                  {s(row, "code")} · {s(row, "name")}
                                </option>
                              ))}
                          </Select>
                          <span
                            id={helpId}
                            className={invalid ? styles.mappingFieldError : styles.mappingFieldHelp}
                          >
                            {invalid
                              ? "Select an account from the selected chart."
                              : "Account used for this POS posting."}
                          </span>
                        </Field>
                      );
                    })}
                  </div>
                </section>
              ))}
              {!data.accounts.some((row) => s(row, "chart_id") === chartId) && (
                <p className={styles.mappingNotice}>
                  No active accounts are available in this chart. Add or activate accounts in{" "}
                  <Link href={`${base}/finance/accounting`}>Finance</Link> first.
                </p>
              )}
            </div>
          </details>
          <div className={styles.settingsActions}>
            <Button type="submit" disabled={busy}>
              Save POS settings
            </Button>
            <span className={styles.muted}>
              Accounting values stay included when this section is closed.
            </span>
          </div>
        </form>
      </FeatureWidget>
      <FormDisclosure className={styles.settingsDisclosure}>
        <summary>
          <span>Registers</span>
          <span className={styles.disclosureSummaryValue}>{registerSummary}</span>
        </summary>
        <div className={styles.disclosureBody}>
          {data.registers.length > 0 && (
            <ul className={styles.registerList}>
              {data.registers.map((row) => (
                <li key={s(row, "id")}>
                  <strong>{s(row, "name")}</strong>
                  <span>
                    {s(row, "mode") === "restaurant" ? "Restaurant" : "Retail"} ·{" "}
                    {s(row, "warehouse_name")}
                  </span>
                  <Tag value={row.active ? "active" : "inactive"} />
                </li>
              ))}
            </ul>
          )}
          <section className={styles.panel}>
            <h2>Add register</h2>
            <form
              className={styles.fields}
              onSubmit={(event) => {
                const f = fields(event);
                void send("register.save", {
                  name: text(f, "name"),
                  warehouseId: text(f, "warehouse"),
                  mode: text(f, "mode"),
                  active: true,
                  expectedVersion: 0,
                });
              }}
            >
              <Field>
                Register name
                <TextInput name="name" required />
              </Field>
              <Field>
                Warehouse
                <Select name="warehouse" required>
                  <option value="">Select a warehouse</option>
                  {data.warehouses.map((row) => (
                    <option key={s(row, "id")} value={s(row, "id")}>
                      {s(row, "name")}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field>
                Register type
                <Select name="mode">
                  <option value="retail">Retail</option>
                  <option value="restaurant">Restaurant</option>
                </Select>
              </Field>
              <Button type="submit" disabled={busy || !data.warehouses.length}>
                Add register
              </Button>
            </form>
            {!data.warehouses.length && (
              <p className={styles.muted}>
                Add a warehouse in <Link href={`${base}/commerce`}>Commerce</Link> first.
              </p>
            )}
          </section>
          {data.registers.some((row) => row.mode === "restaurant") && (
            <section className={`${styles.panel} ${styles.spaced}`}>
              <h2>Restaurant tables</h2>
              <form
                className={styles.fields}
                onSubmit={(event) => {
                  const f = fields(event);
                  void send("table.create", {
                    registerId: text(f, "register"),
                    name: text(f, "name"),
                    seats: Number(f.get("seats")),
                  });
                }}
              >
                <Field>
                  Restaurant register
                  <Select name="register">
                    {data.registers
                      .filter((row) => row.mode === "restaurant")
                      .map((row) => (
                        <option key={s(row, "id")} value={s(row, "id")}>
                          {s(row, "name")}
                        </option>
                      ))}
                  </Select>
                </Field>
                <Field>
                  Table name
                  <TextInput name="name" required />
                </Field>
                <Field>
                  Seats
                  <TextInput type="number" name="seats" min="1" defaultValue="2" required />
                </Field>
                <Button type="submit" disabled={busy}>
                  Add table
                </Button>
              </form>
            </section>
          )}
        </div>
      </FormDisclosure>
      <details className={styles.settingsDisclosure}>
        <summary>
          <span>Products &amp; scanning</span>
          <span className={styles.disclosureSummaryValue}>{productSummary}</span>
        </summary>
        <div className={styles.disclosureBody}>
          <p className={styles.muted}>
            Products come from Commerce. Set barcodes, categories and restaurant menu details here.
          </p>
          {data.products.length ? (
            <Catalog
              data={data}
              busy={busy}
              send={send}
              allowRecipes={data.registers.some((row) => row.mode === "restaurant")}
              api={`/api/v1${base}/pos`}
              refresh={refresh}
            />
          ) : (
            <section className={styles.panel}>
              <h2>No active products</h2>
              <p>
                Add products in <Link href={`${base}/commerce`}>Commerce</Link> before configuring
                barcode details or a restaurant menu.
              </p>
            </section>
          )}
        </div>
      </details>
    </>
  );
}
