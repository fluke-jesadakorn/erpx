"use client";
import {useCallback,useEffect,useRef,useState,useSyncExternalStore} from "react";
import {useSearchParams} from "next/navigation";
import {useMoneyFormatter} from "@/components/money/currency-provider";
import {useFrontLanguage} from "@/components/front/front-language";
import {frontText} from "@/lib/front-language";
import {workFetch} from "@/lib/work-fetch";
import {createLatestRead} from "@/lib/latest-read";
import {createPosSeedReadGate, isLivePosTab, posReadKey, type PosInitialRead} from "@/lib/pos-read-identity";
import type {PosWorkspace} from "@/server/modules/pos/public/pos";
import type {PosRow} from "@/server/modules/pos/public/pos-core";
const s=(row:PosRow,key:string)=>String(row[key]??"");
type Row=PosRow;
function subscribePosConnectivity(onChange:()=>void){ window.addEventListener("online",onChange);window.addEventListener("offline",onChange);return()=>{window.removeEventListener("online",onChange);window.removeEventListener("offline",onChange);}; }
const posConnectivitySnapshot=()=>navigator.onLine;
const posServerConnectivitySnapshot=()=>true;
export type PosConsoleProps = { initial: PosWorkspace; base: string; erpBase?: string; initialTab?: string; frontMode?: boolean; commandScopeKey?: string; frontHref?: string; recordOnly?: boolean; financeMode?: boolean; initialRead?: PosInitialRead; };
export function usePosController({initial,base,initialTab,frontMode=false,commandScopeKey=base,financeMode=false,initialRead}:PosConsoleProps){
  const formatSelectedMoney = useMoneyFormatter();
  const online = useSyncExternalStore(
    subscribePosConnectivity,
    posConnectivitySnapshot,
    posServerConnectivitySnapshot,
  );
  const language = useFrontLanguage(),
    tr = (value: string) => frontText(value, language);
  const searchParams = useSearchParams(),
    requestedTab = searchParams.get("tab") ?? initialTab ?? (financeMode ? "reports" : "sale");
  const maySell = Boolean(initial.permissions["pos.sell"]),
    mayKitchen = Boolean(initial.permissions["pos.kitchen"]),
    mayReview = Boolean(initial.permissions["pos.approve"]);
  const allowedDailyTab =
    requestedTab === "sale"
      ? maySell
      : requestedTab === "kitchen"
        ? mayKitchen || maySell
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
      const query = frontMode
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
    [api, commandScopeKey, tab, cursor, historyQuery, frontMode, financeMode, reads],
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
    // A pre-command poll must not publish or satisfy the post-command refresh.
    reads.cancel();
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
      // Explicit refreshes may have started while the command was committing.
      reads.cancel();
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
  
return {online,language,tr,searchParams,tab,setTab,data,loadedTab,registerId,setRegisterId,error,setError,notice,busy,search,setSearch,category,setCategory,product,setProduct,printing,setPrinting,kitchenStation,setKitchenStation,paymentMethod,setPaymentMethod,setSelected,pendingCommand,cursor,historyQuery,api,currency,money,saleMoney,refresh,send};
}
