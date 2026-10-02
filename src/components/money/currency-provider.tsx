"use client";

import { createContext, useContext, useMemo, useState, useSyncExternalStore, type ReactNode } from "react";
import { currencyScale, convertUnits, convertWithRates, formatMoneyUnits, resolveRate, type CurrencyDefinition, type ManualRate } from "@/lib/company-money";
import { workFetch } from "@/lib/work-fetch";
import { Dialog } from "@/components/ui/dialog";
import { Check, ChevronDown, Search, TrendingUp } from "lucide-react";
import styles from "./currency-provider.module.css";

export type CurrencyContextData = {
  companyId: string;
  defaultCurrency: string;
  displayCurrency: string;
  scales: Record<string, number>;
  rates: ManualRate[];
  currencies: CurrencyDefinition[];
  canConfirmRate: boolean;
};
type CurrencyValue = CurrencyContextData & { setDisplayCurrency: (currency: string) => Promise<void>; refresh: () => Promise<void>; apiBase: string };
const Context = createContext<CurrencyValue | null>(null);
const subscribeHydration = () => () => {};
const clientHydrated = () => true;
const serverHydrated = () => false;

export function CurrencyProvider({ initial, apiBase, children }: { initial: CurrencyContextData; apiBase: string; children: ReactNode }) {
  const [data, setData] = useState(initial);
  async function refresh() {
    const response = await workFetch(`${apiBase}/money`, { cache: "no-store" });
    const body = await response.json() as { data?: CurrencyContextData; error?: { message?: string } };
    if (!response.ok || !body.data) throw new Error(body.error?.message ?? "Could not load currency settings.");
    setData(body.data);
  }
  async function setDisplayCurrency(currency: string) {
    const response = await workFetch(`${apiBase}/money/preference`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() }, body: JSON.stringify({ currency }) });
    const body = await response.json() as { error?: { message?: string } };
    if (!response.ok) throw new Error(body.error?.message ?? "Could not save currency preference.");
    setData((value) => ({ ...value, displayCurrency: currency }));
  }
  return <Context.Provider value={{ ...data, setDisplayCurrency, refresh, apiBase }}>{children}</Context.Provider>;
}

export function useCurrency() {
  const value = useContext(Context);
  if (!value) throw new Error("CurrencyProvider is required.");
  return value;
}

function today() {
  const parts = new Intl.DateTimeFormat("en-GB", { timeZone: "Asia/Bangkok", year: "numeric", month: "2-digit", day: "2-digit" }).formatToParts(new Date());
  const part = (name: string) => parts.find((item) => item.type === name)?.value ?? "";
  return `${part("year")}-${part("month")}-${part("day")}`;
}

export function useMoneyFormatter() {
  const { displayCurrency, rates, scales } = useCurrency();
  return (amountUnits: string | number | bigint, currency: string, date = today(), legacyScale?: number, snapshot?: { usdUnits: string; sourceUnits: string; rateId?: string | null }) => {
    let sourceScale: number;
    try { sourceScale = legacyScale ?? currencyScale(currency, scales[currency]); }
    catch { return formatMoneyUnits(String(amountUnits), currency, legacyScale ?? 2); }
    const original = formatMoneyUnits(String(amountUnits), currency, sourceScale);
    if (currency === displayCurrency) return original;
    let targetScale: number;
    try { targetScale = currencyScale(displayCurrency, scales[displayCurrency]); }
    catch { return `${original} · ${displayCurrency} precision needs setup`; }
    if (snapshot && BigInt(snapshot.sourceUnits) !== BigInt(0)) {
      const numerator = BigInt(amountUnits) * BigInt(snapshot.usdUnits);
      const denominator = BigInt(snapshot.sourceUnits);
      const sameSign = (numerator < BigInt(0)) === (denominator < BigInt(0));
      const absolute = (numerator < BigInt(0) ? -numerator : numerator) / (denominator < BigInt(0) ? -denominator : denominator);
      const remainder = (numerator < BigInt(0) ? -numerator : numerator) % (denominator < BigInt(0) ? -denominator : denominator);
      const usdUnits = (sameSign ? BigInt(1) : BigInt(-1)) * (absolute + (remainder * BigInt(2) >= (denominator < BigInt(0) ? -denominator : denominator) ? BigInt(1) : BigInt(0)));
      if (displayCurrency === "USD") return `${original} · ≈ ${formatMoneyUnits(usdUnits, "USD", 2)} · ข้อมูลประกอบ · เรตตรึงรายการ`;
      const target = resolveRate(rates, displayCurrency, date);
      if (!target) return `${original} · ${displayCurrency} rate unavailable`;
      const frozenSourceRate = currency === "USD" ? "1" : rates.find((item) => item.id === snapshot.rateId)?.currencyPerUsd;
      if (!frozenSourceRate) return `${original} · ${displayCurrency} rate unavailable`;
      const selectedUnits = convertUnits(String(amountUnits), sourceScale, targetScale, frozenSourceRate, target.rate.currencyPerUsd);
      return `${original} · ≈ ${formatMoneyUnits(selectedUnits, displayCurrency, targetScale)} · ข้อมูลประกอบ · เรตตรึงรายการ${target.usedLatestFallback ? " · ใช้เรตล่าสุดแทน" : ""}`;
    }
    let converted: ReturnType<typeof convertWithRates>;
    try { converted = convertWithRates({ amountUnits: String(amountUnits), sourceCurrency: currency, sourceScale,
      targetCurrency: displayCurrency, targetScale, date, rates }); }
    catch { return `${original} · ${displayCurrency} rate unavailable`; }
    if (!converted) return `${original} · ${displayCurrency} rate unavailable`;
    return `${original} · ≈ ${formatMoneyUnits(converted.amountUnits, displayCurrency, targetScale)} · ข้อมูลประกอบ${converted.usedLatestFallback ? " · ใช้เรตล่าสุดแทน" : ""}`;
  };
}

export function MoneyAmount({ units, currency, date, legacyScale, snapshotUsdUnits, snapshotSourceUnits, snapshotRateId, className }: { units: string | number | bigint; currency: string; date?: string; legacyScale?: number; snapshotUsdUnits?: string | null; snapshotSourceUnits?: string | null; snapshotRateId?: string | null; className?: string }) {
  const format = useMoneyFormatter();
  return <span className={className}>{format(units, currency, date, legacyScale, snapshotUsdUnits && snapshotSourceUnits ? { usdUnits: snapshotUsdUnits, sourceUnits: snapshotSourceUnits, rateId: snapshotRateId } : undefined)}</span>;
}

export function CurrencySelector() {
  const context = useCurrency();
  const hydrated = useSyncExternalStore(subscribeHydration, clientHydrated, serverHydrated);
  const [open, setOpen] = useState(false), [search, setSearch] = useState(""), [modal, setModal] = useState(false);
  const [pendingCurrency, setPendingCurrency] = useState(context.displayCurrency);
  const [effectiveDate, setEffectiveDate] = useState(today()), [quote, setQuote] = useState(""), [note, setNote] = useState("");
  const [scale, setScale] = useState("6"), [busy, setBusy] = useState(false), [error, setError] = useState("");
  const visible = useMemo(() => context.currencies.filter((item) => `${item.code} ${item.name}`.toLowerCase().includes(search.toLowerCase())).slice(0, 80), [context.currencies, search]);
  const rate = resolveRate(context.rates, context.displayCurrency, today());
  const pending = context.currencies.find((item) => item.code === pendingCurrency);
  const previous = resolveRate(context.rates, pendingCurrency, effectiveDate);
  const sample = (() => {
    if (!quote || !/^\d{1,20}(?:\.\d{1,18})?$/.test(quote) || !pending) return null;
    try {
      const targetScale = currencyScale(pendingCurrency, context.scales[pendingCurrency] ?? (pending.scale === null ? Number(scale) : null));
      const amount = convertWithRates({ amountUnits: "10000", sourceCurrency: "USD", sourceScale: 2, targetCurrency: pendingCurrency, targetScale, date: effectiveDate,
        rates: [{ id: "preview", currency: pendingCurrency, effectiveDate, currencyPerUsd: quote, confirmedAt: 0 }] });
      return amount ? formatMoneyUnits(amount.amountUnits, pendingCurrency, targetScale) : null;
    } catch { return null; }
  })();
  const beforeSample = (() => {
    if (!previous || !pending) return null;
    try {
      const targetScale = currencyScale(pendingCurrency, context.scales[pendingCurrency] ?? (pending.scale === null ? Number(scale) : null));
      return formatMoneyUnits(convertWithRates({ amountUnits: "10000", sourceCurrency: "USD", sourceScale: 2,
        targetCurrency: pendingCurrency, targetScale, date: effectiveDate, rates: [previous.rate] })!.amountUnits, pendingCurrency, targetScale);
    } catch { return null; }
  })();
  async function saveRate() {
    setError(""); setBusy(true);
    try {
      if (pending?.scale === null && context.scales[pendingCurrency] === undefined) {
        const scaleResponse = await workFetch(`${context.apiBase}/money/scales`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() }, body: JSON.stringify({ currency: pendingCurrency, scale: Number(scale) }) });
        const result = await scaleResponse.json() as { error?: { message?: string } };
        if (!scaleResponse.ok) throw new Error(result.error?.message ?? "Could not configure precision.");
      }
      const response = await workFetch(`${context.apiBase}/money/rates`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() }, body: JSON.stringify({ currency: pendingCurrency, effectiveDate, currencyPerUsd: quote, note }) });
      const result = await response.json() as { error?: { message?: string } };
      if (!response.ok) throw new Error(result.error?.message ?? "Could not confirm the rate.");
      await context.refresh(); setModal(false); setQuote(""); setNote("");
    } catch (cause) { setError(cause instanceof Error ? cause.message : "Could not save rate."); }
    finally { setBusy(false); }
  }
  return <div className={styles.root}>
    <button type="button" className={styles.trigger} aria-label="Choose display currency" aria-expanded={open} aria-haspopup="listbox" disabled={!hydrated} onClick={() => setOpen(!open)}>
      <span className={styles.mark}>$</span><span className={styles.triggerText}><strong>{context.displayCurrency}</strong><small>{rate ? context.displayCurrency === "USD" ? "Company default" : `1 USD = ${rate.rate.currencyPerUsd} · ${rate.rate.effectiveDate}${rate.usedLatestFallback ? " · ใช้เรตล่าสุดแทน" : ""}` : "Rate needed"}</small></span><ChevronDown size={15} aria-hidden="true" />
    </button>
    {open && <><button className={styles.scrim} type="button" aria-label="Close currency menu" onClick={() => setOpen(false)} /><div className={styles.menu} role="listbox" aria-label="Display currency">
      <div className={styles.menuTitle}><span>Currency</span><small>Company default: {context.defaultCurrency}</small></div>
      <label className={styles.search}><Search size={16} aria-hidden="true"/><input autoFocus value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search code or name" aria-label="Search currency" /></label>
      <div className={styles.list}>{visible.map((item) => {
        const activeRate = resolveRate(context.rates, item.code, today());
        return <button type="button" role="option" aria-selected={item.code === context.displayCurrency} className={styles.option} key={item.code} disabled={busy} onClick={async () => {
          setError(""); setBusy(true);
          try { await context.setDisplayCurrency(item.code); setOpen(false); }
          catch (cause) { setError(cause instanceof Error ? cause.message : "Could not change currency."); }
          finally { setBusy(false); }
        }}><span><b>{item.code}</b><small>{item.name}</small></span><span className={styles.optionEnd}>{item.code === context.displayCurrency && <Check size={16} />}{activeRate ? <small>{item.code === "USD" ? "Fixed · 1" : `1 USD = ${activeRate.rate.currencyPerUsd}`}<br/>{item.code !== "USD" && `${activeRate.rate.effectiveDate}${activeRate.usedLatestFallback ? " · ใช้เรตล่าสุดแทน" : " · Confirmed"}`}</small> : <small>Rate needed</small>}</span></button>;
      })}</div>
      {context.canConfirmRate && <button type="button" className={styles.manage} onClick={() => { setPendingCurrency(context.displayCurrency === "USD" ? "THB" : context.displayCurrency); setOpen(false); setModal(true); }}>+ Add or revise manual rate</button>}
      {error && <p className={styles.error} role="alert">{error}</p>}
    </div></>}
    <Dialog open={modal} onClose={() => setModal(false)} title="Confirm manual exchange rate" presentation="responsive-panel">
      <div className={styles.modal}>
        <p className={styles.intro}><TrendingUp size={18} /> One USD equals the amount entered below. This confirmed rate is shared across your company; existing posted amounts stay unchanged.</p>
        <label>Currency<select value={pendingCurrency} onChange={(event) => setPendingCurrency(event.target.value)}>{context.currencies.filter((item) => item.code !== "USD").map((item) => <option key={item.code} value={item.code}>{item.code} · {item.name}</option>)}</select></label>
        {pending?.scale === null && context.scales[pendingCurrency] === undefined && <label>Transaction decimal places<select value={scale} onChange={(event) => setScale(event.target.value)}>{Array.from({ length: 13 }, (_, index) => <option key={index} value={index}>{index}</option>)}</select><small>Locked after setup for this company.</small></label>}
        <label>Effective date<input type="date" value={effectiveDate} onChange={(event) => setEffectiveDate(event.target.value)} required /></label>
        <label>1 USD = <input type="text" inputMode="decimal" value={quote} onChange={(event) => setQuote(event.target.value)} placeholder={`Amount in ${pendingCurrency}`} required /></label>
        <div className={styles.preview}><span>Previous confirmed rate</span><strong>{previous ? `1 USD = ${previous.rate.currencyPerUsd} ${pendingCurrency} · ${previous.rate.effectiveDate}` : "No rate yet"}</strong><span>Before · USD 100.00</span><strong>{beforeSample ?? "—"}</strong><span>After · USD 100.00</span><strong>{sample ?? "Enter a valid rate"}</strong></div>
        <label>Note (optional)<textarea value={note} onChange={(event) => setNote(event.target.value)} maxLength={500} rows={2} placeholder="Source or reason for this manual rate" /></label>
        {error && <p role="alert" className={styles.error}>{error}</p>}
        <div className={styles.actions}><button type="button" className={styles.cancel} onClick={() => setModal(false)}>Cancel</button><button type="button" className={styles.confirm} disabled={busy || !sample || !effectiveDate} onClick={() => void saveRate()}>{busy ? "Saving…" : "Confirm company rate"}</button></div>
      </div>
    </Dialog>
  </div>;
}
