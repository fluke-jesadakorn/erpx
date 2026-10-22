"use client";
import {posReceiptHref} from "@erpx/feature-pos/lib/receipt-route";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {useCallback,useEffect,useLayoutEffect,useRef,useState,useSyncExternalStore} from "react";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import {useMoneyFormatter} from "@erpx/feature-finance/components/money/currency-provider";
import {useFrontLanguage} from "@erpx/front/components/front-language";
import {frontText} from "@erpx/feature-shared/lib/front-language";
import {workFetch} from "@erpx/feature-work_projection/lib/work-fetch";
import {createLatestRead} from "@erpx/feature-shared/lib/latest-read";
import {createPosSeedReadGate, isLivePosTab, posReadKey, type PosInitialRead} from "@erpx/feature-pos/lib/pos-read-identity";
import type {PosWorkspace} from "@erpx/server-pos/public/pos";
import type {PosRow} from "@erpx/server-pos/public/pos-core";
import {posCommandPurchases} from "@erpx/server-pos/public/rules/package-commands";
import {createPosDeviceDraftCommands,type PosDeviceDraftReference,type PosDeviceSource} from '@erpx/feature-pos/lib/device-cart-command';
import type {PosWorkspaceCommandSource,PosWorkspaceCompletion} from '@erpx/feature-pos/lib/workspace-command';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
const s=(row:PosRow,key:string)=>String(row[key]??"");
type Row=PosRow;
function subscribePosConnectivity(onChange:()=>void){ window.addEventListener("online",onChange);window.addEventListener("offline",onChange);return()=>{window.removeEventListener("online",onChange);window.removeEventListener("offline",onChange);}; }
const posConnectivitySnapshot=()=>navigator.onLine;
const posServerConnectivitySnapshot=()=>true;
export type PosConsoleProps = { initial: PosWorkspace; base: string; erpBase?: string; initialTab?: string; frontMode?: boolean; commandScopeKey?: string; frontHref?: string; recordOnly?: boolean; financeMode?: boolean; initialRead?: PosInitialRead; };
export function usePosController({initial,base,initialTab,frontMode=false,commandScopeKey=base,financeMode=false,initialRead,recordOnly=false}:PosConsoleProps){
  const router=useRouter();
  const formatSelectedMoney = useMoneyFormatter();
  const online = useSyncExternalStore(
    subscribePosConnectivity,
    posConnectivitySnapshot,
    posServerConnectivitySnapshot,
  );
  const language = useFrontLanguage(),
    tr = (value: string) => frontText(value, language);
  const searchParams = useSearchParams(),
    requestedTab = recordOnly
      ? initialRead?.tab ?? initialTab ?? (financeMode ? "reports" : "sale")
      : searchParams.get("tab") ?? initialTab ?? (financeMode ? "reports" : "sale");
  const maySell = Boolean(initial.permissions["pos.sell"]),
    mayKitchen = Boolean(initial.permissions["pos.kitchen"]),
    mayReview = Boolean(initial.permissions["pos.approve"]);
  const allowedDailyTab =
    requestedTab === "sale"
      ? maySell
      : requestedTab === "kitchen"
        ? (mayKitchen || maySell) && (initial.purchases?.["pos.kitchen"].history ?? true)
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
    if (recordOnly) return;
    if (value !== tab) setLoadedTab("");
    const url = new URL(window.location.href);
    url.searchParams.set("tab", value);
    url.searchParams.delete("cursor");
    router.push(url.pathname+url.search,{scroll:false});
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
    if (frontMode && value) {
      router.replace(posReceiptHref(window.location.href,value),{scroll:false});
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
  const cursor = recordOnly ? initialRead?.cursor ?? "" : searchParams.get("cursor") ?? "",
    historyQuery = recordOnly ? initialRead?.q ?? "" : searchParams.get("q") ?? "";
  const [reads] = useState(() => createLatestRead<PosWorkspace>());
  const initialKey=initialRead ? posReadKey(base, commandScopeKey, {...initialRead, sale: initialRead.sale ?? (initial.sale ? s(initial.sale, "id") : "")}) : null;
  const [seed,setSeed] = useState(()=>({initial,gate:createPosSeedReadGate(initialKey)}));
  if(seed.initial!==initial){
    setSeed({initial,gate:createPosSeedReadGate(initialKey)});
    setData(initial);
    setLoadedTab(tab);
  }
  const api = `/api/v1${base}/pos`,
    currency = data.currency;
  // A render-local mounted source, separate from the durable original body.
  // The native device branch below retains its existing owner and storage.
  const renderedAssignment=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]')?.dataset.workAssignment;
  const genericSource = useRef<PosWorkspaceCommandSource|null>(null);
  const genericProtocol = useRef<{key:string;commands:ReturnType<typeof import("@erpx/feature-pos/lib/workspace-command").createPosWorkspaceCommands>}|null>(null);
  useLayoutEffect(() => {
    genericSource.current={api,scope:commandScopeKey,userId:data.userId,companyId:data.companyId,branchId:data.branchId,registerId,selection:selected,tab,cursor,q:historyQuery,finance:financeMode,frontMode,recordOnly,data};
    return()=>{genericSource.current=null;};
  },[api,commandScopeKey,data,registerId,selected,tab,cursor,historyQuery,financeMode,frontMode,recordOnly]);
  const [deviceOwner] = useState(() => {
    let readSource:()=>PosDeviceSource|null=()=>null;
    return {commands:createPosDeviceDraftCommands(()=>readSource()),bind(next:()=>PosDeviceSource|null){readSource=next;}};
  });
  const deviceCommands=deviceOwner.commands;
  useLayoutEffect(() => {
    const source={api, scope:commandScopeKey, userId:data.userId, companyId:data.companyId, branchId:data.branchId, registerId, canSell:Boolean(data.permissions['pos.sell'])};
    deviceOwner.bind(()=>({...source,selection:selectionRef.current}));
    return () => {deviceOwner.bind(()=>null);};
  }, [deviceOwner, api, commandScopeKey, data.userId, data.companyId, data.branchId, data.permissions, registerId]);
  useEffect(() => {
    let live=true;
    queueMicrotask(() => {
      if(!live)return;
      try {
        const prior=JSON.parse(sessionStorage.getItem(commandStorageKey)??'null');
        const device=deviceCommands.pending()??deviceCommands.restore(prior);
        const pending=prior&&typeof prior.op==='string'&&typeof prior.key==='string'&&'input' in prior
          ? device&&device.key===prior.key?device:prior
          : device;
        commandRef.current=pending;setPendingCommand(pending);
        if(pending&&!device&&(prior?.kind!=='pos-workspace-command'||prior?.version!==1))setError('This saved POS command has no verified original Company and role. Its original bytes are preserved; it cannot safely be retried under the currently selected Company.');
      } catch(cause) {
        // An unreadable durable submission cannot be replaced with a fresh add.
        setError(cause instanceof Error?cause.message:'Keep the original saved device cart before retrying.');
      }
    });
    return()=>{live=false;};
  }, [commandStorageKey,deviceCommands,registerId]);
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
      const query = frontMode && !recordOnly
        ? (new URL(window.location.href).searchParams.get("q") ?? "")
        : historyQuery;
      const endpoint = `${api}?${new URLSearchParams({ ...(saleId ? { sale: saleId } : {}), ...(financeMode ? {finance:"1"} : {}), q: query, tab, cursor })}`;
      return reads.read(JSON.stringify([commandScopeKey, endpoint]), async signal => {
        const r = await workFetch(endpoint, { cache: "no-store", signal });
        if (!r.headers.get("content-type")?.includes("application/json"))
          throw new Error("โหลดข้อมูล POS ไม่สำเร็จ · POS unavailable. กรุณากด Refresh อีกครั้ง.");
        const body = await r.json();
        if (!r.ok) throw new Error(body.error?.message ?? "Failed to load POS");
        return body.data as PosWorkspace;
      }, updated => {
        if (saleId === selectionRef.current) {
          setData(updated);
          setLoadedTab(tab);
          setError((previous) =>
            [
              "โหลดข้อมูล POS ไม่สำเร็จ กรุณากดรีเฟรชอีกครั้ง",
              "POS data is temporarily unavailable. Try Refresh.",
              "Failed to load POS",
              "โหลดข้อมูล POS ไม่สำเร็จ · POS unavailable. กรุณากด Refresh อีกครั้ง.",
              "POS request timed out. กรุณากดรีเฟรชเพื่อลองใหม่",
            ].includes(previous)
              ? ""
              : previous,
          );
        }
      }).catch(cause => {
        if (cause instanceof Error && cause.name === "TimeoutError")
          throw new Error("POS request timed out. กรุณากดรีเฟรชเพื่อลองใหม่");
        throw cause;
      });
    },
    [api, commandScopeKey, tab, cursor, historyQuery, frontMode, financeMode, recordOnly, reads],
  );
  useEffect(
    () => () => {
      reads.cancel();
    },
    [refresh, reads, seed],
  );
  useEffect(() => {
    selectionRef.current = selected;
    const key = posReadKey(base, commandScopeKey, {sale: selected, q: historyQuery, tab, cursor, finance: financeMode});
    if (!seed.gate.shouldRead(key)) return;
    void refresh(selected).catch((e) => setError(e.message));
  }, [selected, refresh, base, commandScopeKey, historyQuery, tab, cursor, financeMode, seed]);
  useEffect(() => {
    if (!isLivePosTab(tab)) return;
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
    document.addEventListener("visibilitychange", reload);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", reload);
      window.removeEventListener("online", reload);
      document.removeEventListener("visibilitychange", reload);
    };
  }, [refresh, tab]);
  async function send(op: string, input: unknown, select = false, deviceDraft?:PosDeviceDraftReference, completion?:PosWorkspaceCompletion) {
    if (!navigator.onLine) {
      setError(
        language === "th"
          ? "เชื่อมต่ออินเทอร์เน็ตก่อนส่งครัว รับเงิน หรือปิดยอด"
          : "Reconnect before kitchen, payment or closing actions.",
      );
      return undefined;
    }
    let retainedDevice:ReturnType<typeof deviceCommands.pending>;
    try { retainedDevice=deviceCommands.pending()??deviceCommands.restore(commandRef.current); if(retainedDevice&&!commandRef.current){commandRef.current=retainedDevice;setPendingCommand(retainedDevice);} }
    catch(cause){setError(cause instanceof Error?cause.message:'Keep the original device cart before adding more items.');return;}
    // A saved command can already have committed. Keep its retry path and let
    // the owner recover its receipt under current history/authority checks.
    let durableCommand=false;try{durableCommand=sessionStorage.getItem(commandStorageKey)!==null;}catch{durableCommand=true;}
    if (!commandRef.current && !durableCommand && data.purchases) {
      const register = data.registers.find(row => row.id === registerId);
      const denied = posCommandPurchases(op, register?.mode === "restaurant").some(scope => {
        const access = data.purchases?.[scope.capability];
        return !access || !(scope.purpose === "new-work" ? access.newWork : access.settleExisting);
      });
      if (denied) {
        setError(language === "th" ? "แพ็กเกจปัจจุบันไม่รองรับคำสั่งนี้ ตรวจงานค้างหรือกลับ Apps เพื่อเลือกผลิตภัณฑ์" : "The current package does not permit this action. Review existing work or return to Apps.");
        return undefined;
      }
    }
    if (op === "sale.create")
      input = {
        ...(input as Record<string, unknown>),
        currency: (input as Record<string, unknown>).currency ?? data.currency,
      };
    if (busyRef.current) return;
    if(retainedDevice&&commandRef.current&&commandRef.current.key!==retainedDevice.key){setError('Two original POS submissions are retained. Recover their original keys without replacing either saved request.');return;}
    if (deviceDraft || retainedDevice) {
      if(op!=='sale.add'||commandRef.current&&commandRef.current.fingerprint!==op+JSON.stringify(input)){
        setError('Recover the original device add before sending a different command.');return;
      }
      let captured:NonNullable<typeof retainedDevice>;
      try { captured=retainedDevice??deviceCommands.prepare(input,deviceDraft!); }
      catch(cause){setError(cause instanceof Error?cause.message:'Keep the original device add.');return;}
      commandRef.current=captured;setPendingCommand(captured);
      try{sessionStorage.setItem(commandStorageKey,JSON.stringify(captured));}catch{}
      busyRef.current=true;reads.cancel();setBusy(true);setError('');setNotice('');
      try {
        const completed=await deviceCommands.run(captured);completed.assertCurrent();reads.cancel();
        const acknowledged=await completed.acknowledge();completed.assertCurrent();if(!acknowledged)return;
        commandRef.current=null;setPendingCommand(null);try{sessionStorage.removeItem(commandStorageKey);}catch{}
        selectionRef.current=String(completed.receipt.id);setSelected(String(completed.receipt.id));
        setData(completed.view);setLoadedTab('sale');setTab('sale');
        setNotice(acknowledged.newer?(language==='th'?'เพิ่มร่างเดิมแล้ว เก็บรายการที่แก้ไขใหม่ไว้ในร่าง':'Original item added. Your newer draft is retained.'):'Saved');
        announceWorkChange();return completed.receipt;
      } catch(cause) {
        if(deviceCommands.isPending(captured)){commandRef.current=captured;setPendingCommand(captured);try{sessionStorage.setItem(commandStorageKey,JSON.stringify(captured));}catch{}}
        else {commandRef.current=null;setPendingCommand(null);try{sessionStorage.removeItem(commandStorageKey);}catch{}}
        setError(cause instanceof Error?cause.message:'The original device add was not confirmed.');return;
      } finally {busyRef.current=false;setBusy(false);}
    }
    const fingerprint = op + JSON.stringify(input);
    if (commandRef.current && commandRef.current.fingerprint !== fingerprint) {
      setError("Recover the original POS command before submitting newer entries.");
      return;
    }
    // Loading the closed generic protocol is itself asynchronous. Bind this
    // actual click before that await; a context change and return still refuses.
    const clicked=genericSource.current;
    if(!clicked||clicked.userId!==data.userId||clicked.companyId!==data.companyId||clicked.branchId!==data.branchId||clicked.api!==api||clicked.scope!==commandScopeKey||renderedAssignment!==document.querySelector<HTMLElement>('[data-work-revision]')?.dataset.workAssignment){setError('The POS source changed. Review the current owner before sending.');return;}
    const snapshot=()=>JSON.stringify([genericSource.current,window.location.origin,window.location.pathname,window.location.search,document.querySelector<HTMLElement>('[data-work-revision]')?.dataset,document.querySelector<HTMLElement>('[data-front-app]')?.dataset]);
    const clickedForm=completion?.form??document.activeElement?.closest<HTMLFormElement>('form');
    const clickedCompletion:PosWorkspaceCompletion=clickedForm?{...completion,captured:true,form:clickedForm,formEditVersion:clickedForm.getAttribute('data-erpx-edit-version'),fields:JSON.stringify(Array.from(new FormData(clickedForm),([key,value])=>[key,typeof value==='string'?value:{name:value.name,size:value.size,type:value.type,lastModified:value.lastModified}]))}:{...completion,captured:true};
    const stamp=snapshot();let contextChanged=false;
    const contextChange=()=>{contextChanged=true;};
    window.addEventListener('erpx:context-switch-started',contextChange);
    window.addEventListener('erpx:work-context-changed',contextChange);
    const current=()=>{if(contextChanged||genericSource.current!==clicked||snapshot()!==stamp||document.documentElement.dataset.workSwitching==='true')throw Error('The POS source or work role changed. Keep the original request.');};
    busyRef.current=true;reads.cancel();setBusy(true);setError('');setNotice('');
    let protocol:ReturnType<typeof import("@erpx/feature-pos/lib/workspace-command").createPosWorkspaceCommands>|undefined;
    let receiptResult:Record<string,unknown>|undefined,afterAcknowledged:(()=>void|Promise<unknown>)|undefined;
    try {
      const commandsModule=await import('@erpx/feature-pos/lib/workspace-command');current();
      if(!genericProtocol.current||genericProtocol.current.key!==commandStorageKey)genericProtocol.current={key:commandStorageKey,commands:commandsModule.createPosWorkspaceCommands(()=>genericSource.current,commandStorageKey)};
      protocol=genericProtocol.current.commands;
      const result=protocol.run(op,input,select,clickedCompletion);
      try{const pending=protocol.pending();commandRef.current=pending;setPendingCommand(pending);}catch{/* The run below reports the preserved invalid record without an unhandled rejection. */}
      const completed=await result;completed.assertCurrent();reads.cancel();
      const acknowledged=completed.acknowledge();if(!acknowledged)return;
      commandRef.current=null;setPendingCommand(null);
      if(completed.view){setData(completed.view);setLoadedTab(tab);}
      if(completed.receipt.url){window.location.assign(String(completed.receipt.url));return completed.receipt;}
      if(select){selectionRef.current=String(completed.receipt.id);setSelected(String(completed.receipt.id));setTab('sale');}
      else if(op==='sale.create'){selectionRef.current=String(completed.receipt.id);setSelectedState(String(completed.receipt.id));}
      if(op==='settings.bootstrap'&&typeof completed.receipt.registerId==='string')setRegisterId(completed.receipt.registerId);
      setNotice(acknowledged.newer?'Original command saved. Your newer entries are retained.':op==='settings.bootstrap'?`${completed.receipt.mode==='restaurant'?'Restaurant':'Retail'} POS is set up. Open a cash session to start selling.`:'Saved');
      receiptResult=completed.receipt;afterAcknowledged=acknowledged.afterAcknowledged;
    } catch(cause) {
      try{const retained=protocol?.pending();if(retained){commandRef.current=retained;setPendingCommand(retained);}else if(protocol){commandRef.current=null;setPendingCommand(null);}}catch{/* Unbound legacy bytes stay preserved; no inferred owner or new key. */}
      if(genericSource.current)setError(genericSource.current===clicked&&snapshot()===stamp?(cause instanceof Error?cause.message:'The original POS command was not confirmed.'):'The original POS command is retained. Return to its original Company and role to recover it.');
      return undefined;
    } finally {
      window.removeEventListener('erpx:context-switch-started',contextChange);
      window.removeEventListener('erpx:work-context-changed',contextChange);
      busyRef.current=false;if(genericSource.current)setBusy(false);
    }
    // A completed customer constructor may have an original dependent action.
    // Its first receipt stays acknowledged; the next normal send owns a new
    // independently retained attempt and never repeats that constructor.
    if(afterAcknowledged)try{await afterAcknowledged();}catch(cause){if(genericSource.current)setError(cause instanceof Error?cause.message:'The original command was saved. Review its remaining dependent action.');}
    return receiptResult;
  }

return {online,language,tr,searchParams,tab,setTab,data,loadedTab,registerId,setRegisterId,error,setError,notice,busy,search,setSearch,category,setCategory,product,setProduct,printing,setPrinting,kitchenStation,setKitchenStation,paymentMethod,setPaymentMethod,setSelected,pendingCommand,cursor,historyQuery,api,currency,money,saleMoney,refresh,send};
}
