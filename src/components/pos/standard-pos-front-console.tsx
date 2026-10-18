"use client";
import {posReceiptHref} from "@/lib/pos/receipt-route";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import dynamic from "next/dynamic";
import { PosRecordDetail } from "@/components/records/pos-record-detail";
import { RecordDrawer } from "@/components/records/record-drawer";
import { Button as UiButton, Card } from "@/components/ui/foundation";
import { PageHeader } from "@/components/ui/primitives";
import { WorkspaceSections } from "@/components/ui/workspace";
import { usePosController } from "./pos-controller";
import type { PosInitialRead } from "@/lib/pos/pos-read-identity";
import type { PosWorkspace } from "@/server/modules/pos/public/pos";
import type { PosRow } from "@/server/modules/pos/public/pos-core";
import { useState, type ReactNode } from "react";
import styles from "./standard-pos-front-console.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import { posMenu } from "@/components/shared/hospitality/module-menu";
import { PosPackageNotice } from "./pos-package-notice";


type Row = PosRow;


const s = (r: Row, k: string) => String(r[k] ?? "");




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
      appearance="standard"
      variant={secondary ? "secondary" : "primary"}
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





import type {PosSend} from "./standard-pos-sale-workspace";
const FrontPosDailyHistoryReversalsScreen = dynamic(() => import("./pos-front-history-reversals-screen").then(module => module.FrontPosDailyHistoryReversalsScreen));
const FrontPosDailySaleScreen = dynamic(() => import("./pos-front-sale-screen").then(module => module.FrontPosDailySaleScreen));
const FrontPosDailyKitchenScreen = dynamic(() => import("./pos-front-kitchen-screen").then(module => module.FrontPosDailyKitchenScreen));
const FrontPosDailyShiftsScreen = dynamic(() => import("./pos-front-shifts-screen").then(module => module.FrontPosDailyShiftsScreen));
const FrontPosDailyHistoryScreen = dynamic(() => import("./pos-front-history-screen").then(module => module.FrontPosDailyHistoryScreen));
function useFrontPosDailyState({
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
  const [openingDraft, setOpeningDraft] = useState(() => ({
    sourceRegisterId: activeRegisterId, registerId: activeRegisterId, opening: "0",
  }));
  // Keep the opening form across tab/read remounts, but reset after a register switch.
  if (openingDraft.sourceRegisterId !== activeRegisterId) {
    setOpeningDraft({sourceRegisterId: activeRegisterId, registerId: activeRegisterId, opening: "0"});
  }
  const ready = Boolean(
    data.frontReady ??
    (data.settings?.enabled &&
      data.registers.some((row) => Boolean(row.active)) &&
      data.products.length),
  );
  const tabs = [
    ["sale", tr("Sale"), can("pos.sell")],
    ["kitchen", tr("Kitchen"), (can("pos.kitchen") || can("pos.sell")) && (data.purchases?.["pos.kitchen"].history ?? true)],
    ["shifts", tr("Cash shifts"), can("pos.sell") || can("pos.approve")],
    ["history", tr("History"), can("pos.read")],
  ].filter((item) => item[2]) as Array<[string, string, boolean]>;
  const pick = (row: Row) => {
    setSelected(s(row, "id"));
    setRegisterId(s(row, "register_id"));
    window.history.pushState(null,"",posReceiptHref(window.location.href,s(row,"id")));
  };
  return {data, tab, setTab, busy, error, notice, send, refresh, registerId, setSelected, setRegisterId, money, can, approvalHref, erpBase, api, recovery, loading, online, language, tr, kitchenStation, setKitchenStation, sale, activeRegisterId, register, openingDraft, setOpeningDraft, ready, tabs, pick};
}
export type StandardFrontPosDailyContext = ReturnType<typeof useFrontPosDailyState>;
function FrontPosDaily(props: {
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
  const context = useFrontPosDailyState(props);
  const {data, tab, setTab, error, notice, refresh, can, recovery, loading, language, tr, register, ready, tabs} = context;

  return (
    <FrontLocalizedTree>
      <div className={styles.workspace} data-source="live" data-front-daily="pos">
        <PageHeader
          appearance="standard"
          eyebrow={tr("Daily work")}
          title={tr("Point of sale")}
          description={tr("Sell, serve, and close the cash shift")}
          action={<Button secondary onClick={() => void refresh().catch(() => {})}>{tr("Refresh")}</Button>}
        />
        <div className={styles.navigation}>
          <WorkspaceSections
            label={tr("Point of sale")}
            selected={tab}
            items={posMenu(language).filter((item) => tabs.some(([id]) => id === item.id)).map(item => ({id: item.id, title: item.label, description: item.description}))}
            onSelect={setTab}
          />
        </div>
        {tab === "history" && can("pos.approve") && <FrontPosDailyHistoryReversalsScreen context={context}/>}
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
        <PosPackageNotice data={data} />
        {loading ? (
          <Card appearance="standard" className={styles.panel} role={error ? "alert" : "status"} aria-busy={!error}>
            <p className={styles.empty}>
              {error ? (language === "th" ? "โหลดข้อมูลไม่สำเร็จ กรุณากดรีเฟรชเพื่อลองใหม่" : "Could not load POS. Choose Refresh to try again.") : (language === "th" ? "กำลังโหลดข้อมูลจุดขาย…" : "Loading the point of sale…")}
            </p>
          </Card>
        ) : !ready ? (
          <Card appearance="standard" className={styles.setupCard} role="status">
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
          </Card>
        ) : (
          <>
            {tab === "sale" && can("pos.sell") && <FrontPosDailySaleScreen context={context}/>}
            {tab === "kitchen" && (can("pos.kitchen") || can("pos.sell")) && <FrontPosDailyKitchenScreen context={context}/>}
            {tab === "shifts" && (can("pos.sell") || can("pos.approve")) && <FrontPosDailyShiftsScreen context={context}/>}
            {tab === "history" && can("pos.read") && <FrontPosDailyHistoryScreen context={context}/>}
          </>
        )}
      </div>
    </FrontLocalizedTree>
  );
}
export function StandardFrontPosConsole({
  initial,
  base,
  erpBase = base,
  initialTab,
  frontMode = false,
  frontHref,
  commandScopeKey = base,
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
  const {online, tr, searchParams, tab, setTab, data, loadedTab, registerId, setRegisterId, error, notice, busy, setPrinting, setSelected, pendingCommand, api, money, refresh, send} = usePosController({initial, base, initialTab, frontMode, commandScopeKey, financeMode, initialRead, recordOnly});
  const currentTabLoaded = loadedTab === tab;
  const can = (p: string) => Boolean(data.permissions[p]);
  const pickSale = (row: Row) => {
    setSelected(s(row, "id"));
    setRegisterId(s(row, "register_id"));
    window.history.pushState(null,"",posReceiptHref(window.location.href,s(row,"id")));
    setPrinting(false);
  };
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
  const receiptDrawer = searchParams.get("detailKind") === "pos_sale" && searchParams.get("detailId") ? <RecordDrawer title={data.sale ? s(data.sale,"number") : "Receipt"} fullHref={posReceiptHref(frontMode&&frontHref?frontHref+tab:base+"/pos",searchParams.get("detailId")!)}>
    {recovery}
    {error && <p role="alert">{error}<Button onClick={()=>void refresh().catch(()=>{})}>Retry</Button></p>}
    {data.sale && s(data.sale,"id") === searchParams.get("detailId") ? <PosRecordDetail key={s(data.sale,"id")} canEdit={can("pos.sell")} canVerifyTransfer={!frontMode && can("finance.approve")} online={online} data={data} api={api} erpBase={erpBase} busy={busy} send={send} registerId={registerId} setRegisterId={setRegisterId} pickSale={pickSale} openShifts={()=>setTab("shifts")}/> : <p role="status">Loading receipt…</p>}
  </RecordDrawer> : null;
  if (recordOnly) return <div className="ui-stack">{recovery}{error && <p role="alert">{error}<Button onClick={()=>void refresh().catch(()=>{})}>Retry</Button></p>}{data.sale ? <PosRecordDetail key={s(data.sale,"id")} canEdit={can("pos.sell")} canVerifyTransfer={!frontMode && can("finance.approve")} online={online} data={data} api={api} erpBase={erpBase} busy={busy} send={send} registerId={registerId} setRegisterId={setRegisterId} pickSale={pickSale} openShifts={()=>setTab("shifts")}/> : <p role="alert">Receipt unavailable in this work scope.</p>}</div>;
    return (
      <>{receiptDrawer}<FrontPosDaily
        key={`${base}:${commandScopeKey}:${initial.userId}`}
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
        recovery={receiptDrawer ? null : recovery}
        approvalHref={canonicalUiHref(`${erpBase}/pos?tab=history&queue=approval`)}
        erpBase={erpBase}
      /></>
    );
}
