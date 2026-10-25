"use client";
import dynamic from "next/dynamic";
import { PosRecordDetail } from "@/components/records/pos-record-detail";
import { RecordDrawer } from "@/components/records/record-drawer";
import { Button as UiButton, Field, TextInput } from "@/components/ui/foundation";
import { WidgetGrid } from "@/components/features/feature-page";
import { usePosController } from "./pos-controller";
import type { PosInitialRead } from "@/lib/pos-read-identity";
import type { PosWorkspace } from "@/server/modules/pos/public/pos";
import type { PosRow } from "@/server/modules/pos/public/pos-core";
import { type FormEvent, type ReactNode } from "react";
import styles from "./pos.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/front-language";
import { ModuleTiles } from "@/components/hospitality/module-tiles";
import { posMenu } from "@/components/hospitality/module-menu";


const PosReceipt = dynamic(() => import("@/components/pos/pos-receipt").then(module => module.PosReceipt));


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
const BackofficePosConsoleHistoryReversalsScreen = dynamic(() => import("./pos-backoffice-history-reversals-screen").then(module => module.BackofficePosConsoleHistoryReversalsScreen));
const BackofficePosConsoleReadinessScreen = dynamic(() => import("./pos-backoffice-readiness-screen").then(module => module.BackofficePosConsoleReadinessScreen));
const BackofficePosConsoleSaleEmptyScreen = dynamic(() => import("./pos-backoffice-sale-empty-screen").then(module => module.BackofficePosConsoleSaleEmptyScreen));
const BackofficePosConsoleSaleScreen = dynamic(() => import("./pos-backoffice-sale-screen").then(module => module.BackofficePosConsoleSaleScreen));
const BackofficePosConsoleKitchenScreen = dynamic(() => import("./pos-backoffice-kitchen-screen").then(module => module.BackofficePosConsoleKitchenScreen));
const BackofficePosConsoleHistoryScreen = dynamic(() => import("./pos-backoffice-history-screen").then(module => module.BackofficePosConsoleHistoryScreen));
const BackofficePosConsoleShiftsScreen = dynamic(() => import("./pos-backoffice-shifts-screen").then(module => module.BackofficePosConsoleShiftsScreen));
const BackofficePosConsoleReportsScreen = dynamic(() => import("./pos-backoffice-reports-screen").then(module => module.BackofficePosConsoleReportsScreen));
const BackofficePosConsoleSettingsScreen = dynamic(() => import("./pos-backoffice-settings-screen").then(module => module.BackofficePosConsoleSettingsScreen));
const BackofficePosConsoleSaleVoidScreen = dynamic(() => import("./pos-backoffice-sale-void-screen").then(module => module.BackofficePosConsoleSaleVoidScreen));
const BackofficePosConsoleShiftReviewScreen = dynamic(() => import("./pos-backoffice-shift-review-screen").then(module => module.BackofficePosConsoleShiftReviewScreen));
function useBackofficePosConsoleState({
  initial,
  base,
  erpBase = base,
  initialTab,
  frontMode = false,
  commandScopeKey = base,
  frontHref,
  recordOnly = false,
  financeMode = false,
  initialRead,
}: {
  initial: PosWorkspace;
  base: string;
  erpBase?: string;
  initialTab?: string;
  frontMode?: boolean;
  commandScopeKey?: string;
  frontHref?: string;
  recordOnly?: boolean;
  financeMode?: boolean;
  initialRead?: PosInitialRead;
}) {
  const {online, language, tr, searchParams, tab, setTab, data, loadedTab, registerId, setRegisterId, error, setError, notice, busy, search, setSearch, category, setCategory, product, setProduct, printing, setPrinting, kitchenStation, setKitchenStation, paymentMethod, setPaymentMethod, setSelected, pendingCommand, cursor, historyQuery, api, currency, money, saleMoney, refresh, send} = usePosController({initial, base, initialTab, frontMode, commandScopeKey, financeMode, initialRead});
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
    {data.sale && s(data.sale,"id") === searchParams.get("detailId") ? <PosRecordDetail key={s(data.sale,"id")} canEdit={can("pos.sell")} canVerifyTransfer={!frontMode && can("finance.approve")} online={online} data={data} api={api} erpBase={erpBase} busy={busy} send={send} registerId={registerId} setRegisterId={setRegisterId} pickSale={pickSale} openShifts={()=>setTab("shifts")}/> : <p role="status">Loading receipt…</p>}
  </RecordDrawer> : null;
  return {initial, base, erpBase, initialTab, frontMode, commandScopeKey, frontHref, recordOnly, financeMode, initialRead, online, language, tr, searchParams, tab, setTab, data, loadedTab, registerId, setRegisterId, error, setError, notice, busy, search, setSearch, category, setCategory, product, setProduct, printing, setPrinting, kitchenStation, setKitchenStation, paymentMethod, setPaymentMethod, setSelected, pendingCommand, cursor, historyQuery, api, currency, money, saleMoney, refresh, send, currentTabLoaded, can, sale, register, shift, dailyPosRoot, due, saleCommand, pickSale, filtered, recovery, receiptDrawer};
}
export type BackofficePosConsoleContext = ReturnType<typeof useBackofficePosConsoleState>;
export function BackofficePosConsole(props: {
  initial: PosWorkspace;
  base: string;
  erpBase?: string;
  initialTab?: string;
  frontMode?: boolean;
  commandScopeKey?: string;
  frontHref?: string;
  recordOnly?: boolean;
  financeMode?: boolean;
  initialRead?: PosInitialRead;
}) {
  const context = useBackofficePosConsoleState(props);
  const {erpBase, frontMode, recordOnly, financeMode, online, language, tr, tab, setTab, data, registerId, setRegisterId, error, setError, notice, busy, printing, api, refresh, send, currentTabLoaded, can, sale, pickSale, recovery, receiptDrawer} = context;
  
  if (recordOnly) return <div className="ui-stack">{error && <p role="alert">{error}<Button onClick={()=>void refresh().catch(()=>{})}>Retry</Button></p>}{data.sale ? <PosRecordDetail key={s(data.sale,"id")} canEdit={can("pos.sell")} canVerifyTransfer={!frontMode && can("finance.approve")} online={online} data={data} api={api} erpBase={erpBase} busy={busy} send={send} registerId={registerId} setRegisterId={setRegisterId} pickSale={pickSale} openShifts={()=>setTab("shifts")}/> : <p role="alert">Receipt unavailable in this work scope.</p>}</div>;
  return (
    <FrontLocalizedTree>
      {receiptDrawer}
      <div className={styles.workspace} data-source="live">
        {recovery}
        <div className={styles.header}>
          <div>
            <span className={styles.muted}>ERPX · {tr("Point of sale")}</span>
            {financeMode ? <h2>Restaurant accounting</h2> : <h1>{tr("Point of sale")}</h1>}
            <p className={styles.muted}>
              {financeMode ? "Review posting accounts, bank evidence and daily accounting" : tr("Products, accounting and payments connected in one place")}
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
          items={posMenu(language).filter(item => !financeMode || ["reports","settings"].includes(item.id)).filter(
            (item) =>
              (
                ({
                  sale: can("pos.sell"),
                  kitchen: can("pos.kitchen") || can("pos.sell"),
                  shifts: can("pos.sell") || can("pos.approve"),
                  history: can("pos.read"),
                  reports: can("pos.reports"),
                  settings: can("pos.configure") || can("pos.payments.configure") || can("pos.accounting.prepare") || can("pos.accounting.approve"),
                }) as Record<string, boolean>
              )[item.id],
          )}
          onSelect={setTab}
        />
        {!financeMode && can("pos.approve") && tab === "history" && <BackofficePosConsoleHistoryReversalsScreen context={context}/>}
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
            <section className={styles.panel} role={error ? "alert" : "status"} aria-busy={!error}>
              <p className={styles.empty}>
                {error ? (language === "th" ? "โหลดข้อมูลไม่สำเร็จ กรุณากดรีเฟรชเพื่อลองใหม่" : "Could not load POS. Choose Refresh to try again.") : (language === "th" ? "กำลังโหลดข้อมูลจุดขาย…" : "Loading the point of sale…")}
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
              {(tab === "settings" || tab === "sale") && <BackofficePosConsoleReadinessScreen context={context}/>}
              {tab === "sale" &&
                currentTabLoaded &&
                data.settings?.enabled &&
                !data.products.length && <BackofficePosConsoleSaleEmptyScreen context={context}/>}
              {tab === "sale" && <BackofficePosConsoleSaleScreen context={context}/>}
              {tab === "kitchen" && <BackofficePosConsoleKitchenScreen context={context}/>}
              {tab === "history" && <BackofficePosConsoleHistoryScreen context={context}/>}
              {tab === "shifts" && <BackofficePosConsoleShiftsScreen context={context}/>}
              {tab === "reports" && <BackofficePosConsoleReportsScreen context={context}/>}
              {tab === "settings" && <BackofficePosConsoleSettingsScreen context={context}/>}
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
                        {(s(j,"kind")==="refund" ? can("pos.approve") || can("finance.approve") : can("finance.approve")) && (
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
                can("pos.sell") && <BackofficePosConsoleSaleVoidScreen context={{...context, sale}}/>}
              {tab === "shifts" &&
                can("pos.approve") &&
                <BackofficePosConsoleShiftReviewScreen context={context}/>}
            </>
          )}
        </WidgetGrid>
      </div>
    </FrontLocalizedTree>
  );
}

