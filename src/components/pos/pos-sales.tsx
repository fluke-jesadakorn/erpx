"use client";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useEffect, useRef, useState, type FormEvent } from "react";
import type { PosWorkspace } from "@/server/modules/pos/public/pos";
import type { PosRow } from "@/server/modules/pos/public/pos-core";
import type { PosReceiptSnapshot } from "@/server/modules/pos/public/pos-daily";
import { formatPosMoney } from "@/server/modules/pos/public/rules/pos";
import { workFetch } from "@/lib/work-fetch";
import { useFrontLanguage } from "@/components/front/front-language";
import QRCode from "qrcode";
import Image from "next/image";
import styles from "./pos.module.css";
import {SourceSupportLink} from "@/components/modules/source-support-link";
import {TaxDocumentReference} from "@/components/hospitality/tax-document-reference";
type Row = PosRow;
const s = (row: Row, key: string) => String(row[key] ?? "");
const n = (row: Row, key: string) => Number(row[key] ?? 0);
const form = (e: FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  return new FormData(e.currentTarget);
};
const amount = (f: FormData, key: string) => Math.round(Number(f.get(key) ?? 0) * 100);
const quantity = (f: FormData, key: string) => Math.round(Number(f.get(key) ?? 0) * 1000);
const addressText = (value: unknown) => {
  try {
    const address = typeof value === "string" ? JSON.parse(value) : value;
    return address && typeof address === "object"
      ? ["line1", "line2", "city", "region", "postalCode", "countryCode"]
          .map((k) => String(address[k] ?? ""))
          .filter(Boolean)
          .join(" ")
      : "";
  } catch {
    return "";
  }
};
export type PosSend = (
  op: string,
  input: unknown,
  select?: boolean,
) => Promise<Record<string, unknown> | undefined>;
const methodNames: Record<string, [string, string]> = {
  cash: ["เงินสด", "Cash"],
  transfer: ["โอนธนาคาร · รอ Finance ตรวจ", "Bank transfer · Finance verification"],
  edc: ["เครื่องรูดบัตรภายนอก", "External EDC"],
  stripe_card: ["บัตร · Stripe", "Card · Stripe"],
  stripe_promptpay: ["PromptPay · Stripe", "PromptPay · Stripe"],
};

/** Both sale screens recover the same unverified transfer through its owner command. */
export function PosPendingTransfer({payment,data,currency,canVerify,canCancel,send,busy,online}:{
  payment:Row;data:PosWorkspace;currency:string|null;canVerify:boolean;canCancel:boolean;send:PosSend;busy:boolean;online:boolean;
}) {
  const language=useFrontLanguage(),t=(th:string,en:string)=>language==='th'?th:en;
  const money=(value:number)=>formatPosMoney(value,currency);
  return <section className={styles.alert}>
    <strong>{t('บันทึกการโอนแล้ว · รอตรวจสอบ','Bank transfer recorded — awaiting verification')}</strong>
    <p>{t('เลขอ้างอิง','Reference')} {s(payment,'reference')} · {money(n(payment,'amount_minor'))}. {t('บิลยังไม่ชำระจนกว่า Finance จะจับคู่หลักฐานธนาคารที่นำเข้า','This bill is not settled until Finance matches imported bank evidence.')}</p>
    {canVerify&&<form className={styles.fields} onSubmit={event=>{
      const fields=form(event),bank=data.transferEvidence.find(row=>row.id===fields.get('bank'));
      if(bank)void send('payment.verify_transfer',{id:payment.id,expectedVersion:payment.version,bankTransactionId:bank.id,bankVersion:bank.version});
    }}>
      <Field>{t('หลักฐานธนาคารที่นำเข้า','Imported bank evidence')}<Select name="bank" required>
        <option value="">{t('เลือกรายการที่ตรงกัน','Select exact matching row')}</option>
        {data.transferEvidence.map(bank=><option key={s(bank,'id')} value={s(bank,'id')}>{s(bank,'transaction_date')} · {s(bank,'external_id')} · {money(n(bank,'amount_minor'))}</option>)}
      </Select></Field>
      <Button clientManaged type="submit" disabled={busy||!online||!data.transferEvidence.length}>{t('ตรวจยืนยันการโอน','Verify transfer')}</Button>
      {!data.transferEvidence.length&&<p>{t('ยังไม่มีรายการธนาคารที่ตรงกัน ให้นำเข้ารายการธนาคารใน Finance ก่อน','No matching imported bank row. Import the bank statement in Finance first.')}</p>}
    </form>}
    {canCancel&&<Button variant="secondary" type="button" disabled={busy||!online} onClick={()=>void send('payment.cancel_transfer',{id:payment.id,expectedVersion:payment.version})}>{t('ยกเลิกเลขอ้างอิงที่ยังไม่ยืนยัน','Cancel unverified reference')}</Button>}
  </section>;
}

function ProductImage({ api, imageId, name }: { api: string; imageId: string; name: string }) {
  const [url, setUrl] = useState("");
  const frame = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let live = true,
      objectUrl = "",
      started = false;
    if (!imageId || !frame.current) return;
    const controller = new AbortController();
    const load = () => {
      if (started) return;
      started = true;
      // Keep work-context headers and the authenticated thumbnail endpoint.
      void workFetch(`${api}/images/${encodeURIComponent(imageId)}`, {
        cache: "force-cache",
        signal: controller.signal,
      })
        .then(async (response) => {
          if (!response.ok) return;
          objectUrl = URL.createObjectURL(await response.blob());
          if (live) setUrl(objectUrl);
          else URL.revokeObjectURL(objectUrl);
        })
        .catch(() => {});
    };
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          observer.disconnect();
          load();
        }
      },
      { rootMargin: "240px" },
    );
    observer.observe(frame.current);
    return () => {
      live = false;
      observer.disconnect();
      controller.abort();
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [api, imageId]);
  return (
    <div ref={frame} className={styles.productVisual}>
      {url ? (
        <Image
          unoptimized
          src={url}
          alt={name}
          width={480}
          height={480}
          loading="lazy"
          decoding="async"
          className={styles.productImage}
        />
      ) : (
        <div className={styles.imageEmpty} aria-label={name}>
          {name.slice(0, 1)}
        </div>
      )}
    </div>
  );
}
function PaymentQr({ value }: { value: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    if (canvas.current) void QRCode.toCanvas(canvas.current, value, { width: 220, margin: 3 });
  }, [value]);
  return <canvas ref={canvas} aria-label="Payment QR" />;
}
export function PosPromptPayEmailRecovery({
  payment,
  send,
  busy,
  online,
}: {
  payment: Row;
  send: PosSend;
  busy: boolean;
  online: boolean;
}) {
  const language = useFrontLanguage(),
    t = (th: string, en: string) => (language === "th" ? th : en);
  if (
    payment.method !== "stripe_promptpay" ||
    payment.provider_id ||
    !["requested", "exception"].includes(s(payment, "state"))
  )
    return null;
  if (payment.payer_email)
    return (
      <Button
        type="submit"
        variant="primary"
        className={styles.button}
        disabled={busy || !online}
        onClick={() =>
          void send("payment.retry", { id: payment.id, expectedVersion: payment.version })
        }
      >
        {t("ลองสร้าง QR รายการเดิมอีกครั้ง", "Retry existing PromptPay QR")}
      </Button>
    );
  return (
    <form
      className={styles.paymentForm}
      onSubmit={(e) => {
        const f = form(e);
        void send("payment.retry", {
          id: payment.id,
          expectedVersion: payment.version,
          payerEmail: String(f.get("payerEmail")),
        });
      }}
    >
      <p>
        {t(
          "ยังไม่สร้าง QR: กรอกอีเมลผู้จ่ายแล้วลองรายการเดิมอีกครั้ง",
          "QR not created: enter the payer email and retry this payment.",
        )}
      </p>
      <Field>
        {t("อีเมลผู้จ่าย PromptPay", "PromptPay payer email")}
        <TextInput name="payerEmail" type="email" required />
      </Field>
      <Button clientManaged type="submit" variant="primary" className={styles.button} disabled={busy || !online}>
        {t("บันทึกอีเมลและสร้าง QR รายการเดิม", "Save email and retry existing QR")}
      </Button>
    </form>
  );
}

export function PosReceipt({ receipt }: { receipt: PosReceiptSnapshot | null }) {
  const language = useFrontLanguage(),
    t = (th: string, en: string) => (language === "th" ? th : en);
  if (!receipt) return null;
  const money = (v: number) => formatPosMoney(v, receipt.currency);
  return (
    <div className={styles.printArea}>
      <article className={styles.receipt} data-pos-receipt={receipt.saleId}>
        <h2>{t("ใบเสร็จรับเงิน", "Receipt")}</h2>
        <strong>{String(receipt.issuer.companyName ?? "")}</strong>
        <p>
          {String(receipt.issuer.registerName ?? "")}
          <br />
          {receipt.number}
          <br />
          {receipt.businessDate}
        </p>
        {receipt.customer && (
          <p>
            {t("ลูกค้า", "Customer")}: {s(receipt.customer, "name")}
            <br />
            {s(receipt.customer, "phone")}
            {s(receipt.customer, "notification_email") && (
              <>
                <br />
                {s(receipt.customer, "notification_email")}
              </>
            )}
            {addressText(receipt.customer.address_json) && (
              <>
                <br />
                {addressText(receipt.customer.address_json)}
              </>
            )}
          </p>
        )}
        {receipt.lines.map((l) => (
          <div className={styles.row} key={s(l, "id")}>
            <span>
              {s(l, "name")} × {n(l, "quantity_milli") / 1000}
              {s(l, "note") && <small>{s(l, "note")}</small>}
            </span>
            <span>{money(n(l, "subtotal_minor") + n(l, "tax_minor"))}</span>
          </div>
        ))}
        <div className={styles.row}>
          <span>{t("ส่วนลด", "Discount")}</span>
          <span>{money(receipt.discountMinor)}</span>
        </div>
        <div className={styles.row}>
          <span>{t("ยอดก่อนภาษี", "Subtotal")}</span>
          <span>{money(receipt.subtotalMinor)}</span>
        </div>
        <div className={styles.row}>
          <span>{t("ภาษี", "Tax")}</span>
          <span>{money(receipt.taxMinor)}</span>
        </div>
        <div className={styles.total}>
          <strong>{t("รวมสุทธิ", "Total")}</strong>
          <strong>{money(receipt.totalMinor)}</strong>
        </div>
        {receipt.payments.map((p) => (
          <p key={s(p, "id")}>
            {methodNames[s(p, "method")]?.[language === "th" ? 0 : 1] ?? s(p, "method")}{" "}
            {money(n(p, "amount_minor"))}
            {p.method === "cash" && (
              <>
                <br />
                {t("รับเงิน", "Tendered")} {money(n(p, "tendered_minor"))} ·{" "}
                {t("เงินทอน", "Change")} {money(n(p, "tendered_minor") - n(p, "amount_minor"))}
              </>
            )}
          </p>
        ))}
        {receipt.roomCharge && (
          <p>
            {t("ลงบัญชีห้องพัก · ชำระตอนเช็กเอาต์", "Charged to hotel folio · Pay at checkout")}
          </p>
        )}
        <p>{t("เอกสารนี้ไม่ใช่ใบกำกับภาษี", "This document is not a tax invoice")}</p>
      </article>
    </div>
  );
}

type Draft = {
  key: string;
  productId: string;
  name: string;
  quantityMilli: number;
  modifierIds: string[];
  note: string;
  discountMinor: number;
  expectedCatalogVersion: number;
  expectedPriceMinor: number;
  saleId: string | null;
  saleVersion: number | null;
};
export function PosSaleWorkspace({
  data,
  api,
  erpBase,
  busy,
  send,
  registerId,
  setRegisterId,
  pickSale,
  openShifts,
}: {
  data: PosWorkspace;
  api: string;
  erpBase: string;
  busy: boolean;
  send: PosSend;
  registerId: string;
  setRegisterId: (id: string) => void;
  pickSale: (row: Row) => void;
  openShifts: () => void;
}) {
  const language = useFrontLanguage(),
    t = (th: string, en: string) => (language === "th" ? th : en);
  const catalogRef = useRef<HTMLElement>(null),
    cartRef = useRef<HTMLElement>(null),
    selectionRef = useRef<HTMLFormElement>(null);
  const [search, setSearch] = useState(""),
    [category, setCategory] = useState(""),
    [chosen, setChosen] = useState<Row | null>(null),
    [customerQuery, setCustomerQuery] = useState(""),
    [customers, setCustomers] = useState<Row[]>(data.customers),
    [customerId, setCustomerId] = useState(""),
    [newCustomer, setNewCustomer] = useState(false),
    [localError, setLocalError] = useState(""),
    [method, setMethod] = useState("cash"),
    [online, setOnline] = useState(true),
    [drafts, setDrafts] = useState<Draft[]>([]),
    [loadedDraftKey, setLoadedDraftKey] = useState("");
  const sale = data.sale,
    register = data.registers.find((r) => r.id === registerId),
    shift = data.shifts.find((r) => r.register_id === registerId && r.state === "open");
  const money = (v: number) => formatPosMoney(v, sale ? s(sale, "currency") : data.currency);
  const editable =
    !!sale &&
    ["open", "held"].includes(s(sale, "state")) &&
    data.payments.every((p) => ["cancelled", "failed"].includes(s(p, "state")));
  const paid = data.payments
      .filter((p) => p.state === "succeeded")
      .reduce((sum, p) => sum + n(p, "amount_minor") - n(p, "refunded_minor"), 0),
    due = Math.max(0, (sale ? n(sale, "total_minor") : 0) - paid);
  const postingPending =
    sale?.state !== "completed" && data.payments.some((p) => Boolean(p.provider_payment_confirmed));
  const waiting = data.payments
    .filter((p) => ["requested", "pending", "exception"].includes(s(p, "state")))
    .reduce((sum, p) => sum + n(p, "amount_minor") - n(p, "refunded_minor"), 0);
  const products = data.products.filter((p) =>
    JSON.parse(s(p, "register_ids_json") || "[]").includes(registerId),
  );
  const available = (p: Row) =>
    Number(JSON.parse(s(p, "availability_json") || "{}")[registerId] ?? 0);
  useEffect(() => {
    if (chosen) selectionRef.current?.scrollIntoView({ block: "center" });
  }, [chosen]);
  const visible = products.filter(
    (p) =>
      (!category || p.category === category) &&
      `${p.name} ${p.sku} ${p.barcode ?? ""}`.toLowerCase().includes(search.toLowerCase()),
  );
  const storageKey = `erpx-pos-draft:${data.userId}:${data.companyId}:${data.branchId ?? "company"}:${registerId}`;
  useEffect(() => {
    let live = true;
    queueMicrotask(() => {
      if (!live) return;
      try {
        const saved = JSON.parse(localStorage.getItem(storageKey) ?? "[]");
        setDrafts(
          Array.isArray(saved)
            ? saved.filter(
                (d) =>
                  d &&
                  typeof d.key === "string" &&
                  typeof d.productId === "string" &&
                  Number.isSafeInteger(d.quantityMilli) &&
                  d.quantityMilli > 0 &&
                  Number.isSafeInteger(d.expectedCatalogVersion) &&
                  Number.isSafeInteger(d.expectedPriceMinor) &&
                  Array.isArray(d.modifierIds) &&
                  d.modifierIds.every((id: unknown) => typeof id === "string"),
              )
            : [],
        );
      } catch {
        setDrafts([]);
      }
      setLoadedDraftKey(storageKey);
    });
    return () => {
      live = false;
    };
  }, [storageKey]);
  useEffect(() => {
    if (loadedDraftKey === storageKey)
      try {
        localStorage.setItem(storageKey, JSON.stringify(drafts));
      } catch {}
  }, [loadedDraftKey, drafts, storageKey]);
  useEffect(() => {
    const update = () => setOnline(navigator.onLine);
    queueMicrotask(update);
    window.addEventListener("online", update);
    window.addEventListener("offline", update);
    return () => {
      window.removeEventListener("online", update);
      window.removeEventListener("offline", update);
    };
  }, []);
  const activeCustomerId = String(sale?.party_id ?? customerId);
  useEffect(() => {
    if (!online) return;
    let live = true;
    const timer = setTimeout(() => {
      void workFetch(`${api}/customers/search?q=${encodeURIComponent(customerQuery)}`)
        .then(async (r) => {
          if (!r.ok || !live) return;
          const rows: Row[] = (await r.json()).data;
          setCustomers((previous) => {
            const selected = previous.find((row) => row.id === activeCustomerId);
            return selected && !rows.some((row) => row.id === selected.id)
              ? [selected, ...rows]
              : rows;
          });
        })
        .catch(() => {});
    }, 250);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [api, customerQuery, online, activeCustomerId]);
  const command = (op: string, extra: Record<string, unknown> = {}) =>
    sale
      ? send(op, { id: sale.id, expectedVersion: sale.version, ...extra })
      : Promise.resolve(undefined);
  async function createSale(tableId: string | null = null) {
    if (!shift) return undefined;
    return send(
      "sale.create",
      {
        shiftId: shift.id,
        serviceMode:
          register?.mode === "restaurant" ? (tableId ? "dine_in" : "takeaway") : "retail",
        tableId,
        partyId: customerId || null,
        guests: 1,
      },
      true,
    );
  }
  async function addProduct(
    product: Row,
    input: { quantityMilli: number; modifierIds: string[]; discountMinor: number; note: string },
  ) {
    setLocalError("");
    const draft: Draft = {
      key: crypto.randomUUID(),
      productId: s(product, "id"),
      name: s(product, "name"),
      expectedPriceMinor: n(product, "price_minor"),
      expectedCatalogVersion: n(product, "catalog_version"),
      saleId: sale ? s(sale, "id") : null,
      saleVersion: sale ? n(sale, "version") : null,
      ...input,
    };
    if (!online) {
      setDrafts((rows) => [...rows, draft]);
      setChosen(null);
      return;
    }
    let saleId = sale?.id,
      version = sale?.version;
    if (!sale) {
      const created = await createSale();
      if (!created) return;
      saleId = String(created.id);
      version = 1;
    }
    const result = await send(
      "sale.add",
      {
        id: saleId,
        expectedVersion: version,
        ...input,
        productId: product.id,
        expectedPriceMinor: draft.expectedPriceMinor,
        expectedCatalogVersion: draft.expectedCatalogVersion,
      },
      true,
    );
    if (result) setChosen(null);
  }
  const staleDrafts = drafts.some((d) => {
    const p = products.find((p) => p.id === d.productId);
    return (
      !p ||
      n(p, "catalog_version") !== d.expectedCatalogVersion ||
      n(p, "price_minor") !== d.expectedPriceMinor ||
      (d.saleId && d.saleId !== sale?.id) ||
      (d.saleVersion !== null && d.saleVersion !== sale?.version)
    );
  });
  async function sendDrafts() {
    if (!online || staleDrafts) return;
    let saleId = sale?.id,
      version = sale?.version;
    const requested = new Map<string, number>();
    for (const draft of drafts)
      requested.set(draft.productId, (requested.get(draft.productId) ?? 0) + draft.quantityMilli);
    if (
      [...requested].some(([id, qty]) => {
        const p = products.find((p) => p.id === id);
        return !p || available(p) < qty;
      })
    ) {
      setLocalError(
        t(
          "สต็อกไม่พอสำหรับร่างทั้งหมด ลดจำนวนหรือตรวจสต็อกอีกครั้ง",
          "Insufficient stock for these drafts. Reduce quantities or refresh stock.",
        ),
      );
      return;
    }
    if (!sale) {
      const created = await createSale();
      if (!created) return;
      saleId = String(created.id);
      version = 1;
    }
    for (const draft of drafts) {
      const { key: _key, name: _name, saleId: _sale, saleVersion: _version, ...input } = draft;
      void _key;
      void _name;
      void _sale;
      void _version;
      const result = await send(
        "sale.add",
        { id: saleId, expectedVersion: version, ...input },
        true,
      );
      if (!result) break;
      version = Number(result.version);
      setDrafts((rows) => rows.filter((d) => d.key !== draft.key));
    }
  }
  const pendingSales = data.sales.filter(
    (sale) =>
      sale.register_id === registerId &&
      ["open", "held", "payment_pending"].includes(s(sale, "state")),
  );
  return (
    <>
      <div className={styles.registerBar}>
        <Field>
          {t("จุดขาย", "Register")}
          <Select
            aria-label={t("จุดขาย", "Register")}
            value={registerId}
            onChange={(e) => setRegisterId(e.target.value)}
            disabled={!!sale && ["open", "held", "payment_pending"].includes(s(sale, "state"))}
          >
            {data.registers
              .filter((r) => r.active)
              .map((r) => (
                <option key={s(r, "id")} value={s(r, "id")}>
                  {s(r, "name")}
                </option>
              ))}
          </Select>
        </Field>
        <span>
          {register?.mode === "restaurant" ? t("ร้านอาหาร", "Restaurant") : t("ค้าปลีก", "Retail")}{" "}
          · {data.currency}
        </span>
        {!shift && (
          <Button type="submit" variant="primary" className={styles.button} onClick={openShifts}>
            {t("เปิดกะก่อนขาย", "Open cash shift")}
          </Button>
        )}
        <Button
          type="submit"
          variant="secondary"
          className={`${styles.button} ${styles.secondary}`}
          disabled={busy || !online || !shift || postingPending}
          onClick={() => void createSale()}
        >
          {register?.mode === "restaurant"
            ? t("บิลใหม่ / ซื้อกลับ", "New sale / takeaway")
            : t("บิลใหม่", "New sale")}
        </Button>
      </div>
      {localError && (
        <p role="alert" className={styles.alert}>
          {localError}
        </p>
      )}
      {!online && (
        <p role="status" className={styles.alert}>
          {t(
            "ออฟไลน์ · เพิ่มสินค้าเป็นร่างในเครื่องได้ รับเงินและปิดยอดเมื่อออนไลน์เท่านั้น",
            "Offline · cart drafts stay on this device. Reconnect before payment or closing.",
          )}
        </p>
      )}
      <div className={styles.saleLayout}>
        <section ref={catalogRef} className={styles.catalogPane}>
          {register?.mode === "restaurant" && (
            <details open className={styles.panel}>
              <summary>{t("ผังโต๊ะ", "Tables")}</summary>
              <div className={styles.tableMap}>
                {data.tables
                  .filter((table) => table.register_id === registerId)
                  .map((table) => {
                    const occupied = pendingSales.find((sale) => sale.table_id === table.id);
                    return (
                      <Button
                        type="submit"
                        variant="plain"
                        className={`${styles.tableTile} ${occupied ? styles.occupied : ""}`}
                        key={s(table, "id")}
                        disabled={busy || !online || !shift}
                        onClick={() =>
                          occupied ? pickSale(occupied) : void createSale(s(table, "id"))
                        }
                      >
                        <strong>{s(table, "name")}</strong>
                        <small>
                          {occupied
                            ? `${occupied.state === "payment_pending" ? t("รอรับเงิน", "Payment pending") : t("กำลังใช้", "Occupied")} · ${money(n(occupied, "total_minor"))}`
                            : t("ว่าง", "Available")}
                        </small>
                      </Button>
                    );
                  })}
              </div>
            </details>
          )}
          {!!pendingSales.length && (
            <details className={styles.panel}>
              <summary>
                {t("บิลเปิด / พักบิล", "Open / held sales")} ({pendingSales.length})
              </summary>
              <div className={styles.actions}>
                {pendingSales.map((row) => (
                  <Button
                    type="submit"
                    variant="secondary"
                    key={s(row, "id")}
                    className={`${styles.button} ${styles.secondary}`}
                    onClick={() => pickSale(row)}
                  >
                    {s(row, "number")} · {money(n(row, "total_minor"))}
                  </Button>
                ))}
              </div>
            </details>
          )}
          <section className={styles.panel}>
            <h2>
              {register?.mode === "restaurant"
                ? t("สินค้าและเมนู", "Products & menu")
                : t("สินค้า", "Products")}
            </h2>
            <form
              className={styles.scanBar}
              onSubmit={(e) => {
                e.preventDefault();
                const product = products.find((p) => p.barcode === search || p.sku === search);
                if (product)
                  void addProduct(product, {
                    quantityMilli: 1000,
                    modifierIds: [],
                    discountMinor: 0,
                    note: "",
                  }).then(() => setSearch(""));
              }}
            >
              <TextInput
                autoComplete="off"
                aria-label={t("ค้นหา / สแกนบาร์โค้ด", "Search / scan barcode")}
                placeholder={t(
                  "ค้นหาชื่อสินค้า SKU หรือสแกนบาร์โค้ด",
                  "Search name, SKU or scan barcode",
                )}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
              <Button
                clientManaged type="submit"
                variant="primary"
                className={styles.button}
                disabled={busy || (!shift && online)}
              >
                {t("เพิ่ม", "Add")}
              </Button>
            </form>
            <div className={styles.categoryBar}>
              <Button
                type="submit"
                variant="plain"
                className={styles.category}
                aria-pressed={!category}
                onClick={() => setCategory("")}
              >
                {t("ทั้งหมด", "All")}
              </Button>
              {[...new Set(products.map((p) => s(p, "category")).filter(Boolean))].map((c) => (
                <Button
                  type="submit"
                  variant="plain"
                  className={styles.category}
                  aria-pressed={category === c}
                  key={c}
                  onClick={() => setCategory(c)}
                >
                  {c}
                </Button>
              ))}
            </div>
            {chosen && (
              <form
                ref={selectionRef}
                className={styles.selectionCard}
                onSubmit={(e) => {
                  const f = form(e);
                  void addProduct(chosen, {
                    quantityMilli: quantity(f, "quantity"),
                    modifierIds: f.getAll("modifier").map(String),
                    discountMinor: amount(f, "discount"),
                    note: String(f.get("note") ?? ""),
                  });
                }}
              >
                <div className={styles.header}>
                  <h3>{s(chosen, "name")}</h3>
                  <Button
                    variant="secondary"
                    type="button"
                    className={`${styles.button} ${styles.secondary}`}
                    onClick={() => setChosen(null)}
                  >
                    {t("ปิด", "Close")}
                  </Button>
                </div>
                <div className={styles.fields}>
                  <Field>
                    {t("จำนวน", "Quantity")}
                    <TextInput
                      name="quantity"
                      type="number"
                      step="0.001"
                      min="0.001"
                      defaultValue="1"
                      required
                    />
                  </Field>
                  <Field>
                    {t("ส่วนลดรายการ", "Line discount")}
                    <TextInput name="discount" type="number" step="0.01" min="0" defaultValue="0" />
                  </Field>
                  <Field>
                    {t("หมายเหตุ", "Notes")}
                    <TextInput name="note" maxLength={500} />
                  </Field>
                </div>
                <div className={styles.modifierChoices}>
                  {(
                    JSON.parse(s(chosen, "modifiers_json") || "[]") as Array<{
                      id: string;
                      name: string;
                      priceMinor: number;
                    }>
                  ).map((m) => (
                    <Field orientation="inline" key={m.id}>
                      <TextInput type="checkbox" name="modifier" value={m.id} />
                      {m.name} +{money(m.priceMinor)}
                    </Field>
                  ))}
                </div>
                <Button
                  clientManaged type="submit"
                  variant="primary"
                  className={styles.button}
                  disabled={busy || (!!sale && !editable)}
                >
                  {t("เพิ่มเข้าตะกร้า", "Add to cart")}
                </Button>
              </form>
            )}
            <div className={styles.products}>
              {visible.map((product) => (
                <Button
                  variant="plain"
                  type="button"
                  className={styles.product}
                  key={s(product, "id")}
                  disabled={
                    busy ||
                    (online && !shift) ||
                    (online && available(product) <= 0) ||
                    (!!sale && !editable)
                  }
                  onClick={() => setChosen(product)}
                >
                  <ProductImage
                    api={api}
                    imageId={s(product, "image_id")}
                    name={s(product, "name")}
                  />
                  <strong>{s(product, "name")}</strong>
                  <span className={styles.productPrice}>{money(n(product, "price_minor"))}</span>
                  <span className={styles.productTax}>
                    {product.tax_inclusive
                      ? t("รวมภาษีแล้ว", "Tax included")
                      : t("ยังไม่รวมภาษี", "Before tax")}
                  </span>
                  <small>
                    {s(product, "sku")} ·{" "}
                    {available(product) >= 999999999
                      ? t("บริการ", "Service")
                      : `${t("พร้อมขาย", "Available")} ${new Intl.NumberFormat(language === "th" ? "th-TH" : "en-US", { maximumFractionDigits: 3 }).format(available(product) / 1000)}`}
                  </small>
                </Button>
              ))}
            </div>
            {!visible.length && (
              <p>
                {t(
                  "ไม่มีสินค้าที่เปิดขายในจุดขายนี้",
                  "No enabled products in this register catalog",
                )}
              </p>
            )}
          </section>
        </section>
        <aside
          ref={cartRef}
          className={`${styles.panel} ${styles.cartPane}`}
          aria-label={t("ตะกร้าและชำระเงิน", "Cart & payment")}
        >
          <div className={styles.header}>
            <h2>{t("ตะกร้า", "Cart")}</h2>
            {sale && <span className={styles.tag}>{s(sale, "state")}</span>}
          </div>
          {sale && <><p className={styles.muted}>{s(sale, "number")}</p><SourceSupportLink base={api.replace(/^\/api\/v1/,"").replace(/\/pos$/,"")} kind="pos_sale" id={s(sale,"id")} version={n(sale,"version")} label={t("แจ้งปัญหาบิลนี้","Report an issue with this bill")}/></>}
          <FormDisclosure className={styles.customerPicker}>
            <summary>
              {t("ลูกค้า", "Customer")} ·{" "}
              {s(customers.find((c) => c.id === (sale?.party_id ?? customerId)) ?? {}, "name") ||
                t("ลูกค้าทั่วไป", "Walk-in")}
            </summary>
            <TextInput
              aria-label={t("ค้นหาลูกค้า CRM", "Search CRM customers")}
              placeholder={t("ชื่อ เบอร์โทร หรืออีเมล", "Name, phone or email")}
              value={customerQuery}
              onChange={(e) => setCustomerQuery(e.target.value)}
            />
            <Select
              aria-label={t("เลือกลูกค้า", "Select customer")}
              value={sale?.party_id ? String(sale.party_id) : customerId}
              onChange={(e) => {
                setCustomerId(e.target.value);
                if (sale) void command("sale.customer", { partyId: e.target.value || null });
              }}
              disabled={busy || (!!sale && !editable) || !online}
            >
              <option value="">{t("ลูกค้าทั่วไป", "Walk-in")}</option>
              {customers.map((c) => (
                <option key={s(c, "id")} value={s(c, "id")}>
                  {s(c, "name")} {s(c, "phone")}
                </option>
              ))}
            </Select>
            <Button
              variant="secondary"
              type="button"
              className={`${styles.button} ${styles.secondary}`}
              onClick={() => setNewCustomer(!newCustomer)}
            >
              {t("เพิ่มลูกค้าใหม่", "New customer")}
            </Button>
            {newCustomer && (
              <form
                className={styles.fields}
                onSubmit={(e) => {
                  const f = form(e);
                  void send("customer.create", {
                    name: String(f.get("name")),
                    phone: String(f.get("phone") ?? ""),
                    email: String(f.get("email") ?? ""),
                    address: f.get("address")
                      ? {
                          line1: String(f.get("address")),
                          city: String(f.get("city") ?? ""),
                          postalCode: String(f.get("postal") ?? ""),
                          countryCode: "TH",
                        }
                      : null,
                  }).then(async (result) => {
                    if (!result) return;
                    const customer = { id: String(result.id), name: String(result.name) };
                    setCustomers((rows) => [customer, ...rows]);
                    setCustomerId(customer.id);
                    setNewCustomer(false);
                    if (sale) await command("sale.customer", { partyId: customer.id });
                  });
                }}
              >
                <Field>
                  {t("ชื่อ", "Name")}
                  <TextInput name="name" required maxLength={200} />
                </Field>
                <Field>
                  {t("โทรศัพท์", "Phone")}
                  <TextInput name="phone" type="tel" maxLength={50} />
                </Field>
                <Field>
                  {t("อีเมล", "Email")}
                  <TextInput name="email" type="email" maxLength={320} />
                </Field>
                <Field>
                  {t("ที่อยู่", "Address")}
                  <TextInput name="address" maxLength={200} />
                </Field>
                <Field>
                  {t("จังหวัด / เมือง", "City")}
                  <TextInput name="city" maxLength={100} />
                </Field>
                <Field>
                  {t("รหัสไปรษณีย์", "Postal code")}
                  <TextInput name="postal" maxLength={30} />
                </Field>
                <Button
                  clientManaged type="submit"
                  variant="primary"
                  className={styles.button}
                  disabled={busy || !online}
                >
                  {t("บันทึกลูกค้า", "Save customer")}
                </Button>
              </form>
            )}
          </FormDisclosure>
          {!sale && !drafts.length && (
            <p className={styles.empty}>
              {register?.mode === "restaurant"
                ? t(
                    "เลือกโต๊ะหรือเพิ่มสินค้าเพื่อเปิดบิล",
                    "Choose a table or add a product to begin",
                  )
                : t(
                    "เลือกสินค้าหรือสแกนบาร์โค้ดเพื่อเริ่มบิล",
                    "Choose a product or scan a barcode to begin",
                  )}
            </p>
          )}
          {data.lines.map((line) => (
            <article key={s(line, "id")} className={styles.cartLine}>
              <div className={styles.row}>
                <strong>
                  {s(line, "name")} × {n(line, "quantity_milli") / 1000}
                </strong>
                <span>{money(n(line, "subtotal_minor") + n(line, "tax_minor"))}</span>
              </div>
              <small>
                {s(line, "note")} · {s(line, "fulfillment_state")}
              </small>
              {editable && (
                <FormDisclosure>
                  <summary>{t("แก้ไข / แยกบิล", "Edit / split")}</summary>
                  <form
                    className={styles.fields}
                    onSubmit={(e) => {
                      const f = form(e);
                      void command("sale.update_line", {
                        lineId: line.id,
                        quantityMilli: quantity(f, "quantity"),
                        discountMinor: amount(f, "discount"),
                        note: String(f.get("note") ?? ""),
                      });
                    }}
                  >
                    <Field>
                      {t("จำนวน", "Quantity")}
                      <TextInput
                        name="quantity"
                        type="number"
                        step="0.001"
                        min="0.001"
                        defaultValue={n(line, "quantity_milli") / 1000}
                      />
                    </Field>
                    <Field>
                      {t("ส่วนลดรายการ", "Line discount")}
                      <TextInput
                        name="discount"
                        type="number"
                        step="0.01"
                        min="0"
                        defaultValue={
                          (n(line, "discount_minor") - n(line, "bill_discount_minor")) / 100
                        }
                      />
                    </Field>
                    <Field>
                      {t("หมายเหตุ", "Notes")}
                      <TextInput name="note" defaultValue={s(line, "note")} />
                    </Field>
                    <Button
                      clientManaged type="submit"
                      variant="primary"
                      className={styles.button}
                      disabled={busy || !online}
                    >
                      {t("บันทึก", "Save")}
                    </Button>
                    <Button
                      variant="secondary"
                      type="button"
                      className={`${styles.button} ${styles.secondary}`}
                      disabled={busy || !online}
                      onClick={() => void command("sale.remove", { lineId: line.id })}
                    >
                      {t("ลบรายการ", "Remove")}
                    </Button>
                  </form>
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      const f = form(e);
                      void command("sale.split", {
                        lineId: line.id,
                        quantityMilli: quantity(f, "quantity"),
                      }).then((result) => {
                        if (result) pickSale({ id: String(result.id), register_id: registerId });
                      });
                    }}
                  >
                    <Field>
                      {t("จำนวนที่แยก", "Quantity to split")}
                      <TextInput
                        name="quantity"
                        type="number"
                        step="0.001"
                        min="0.001"
                        max={n(line, "quantity_milli") / 1000}
                        defaultValue={n(line, "quantity_milli") / 1000}
                      />
                    </Field>
                    <Button
                      clientManaged type="submit"
                      variant="secondary"
                      className={`${styles.button} ${styles.secondary}`}
                      disabled={busy || !online}
                    >
                      {t("แยกบิล", "Split")}
                    </Button>
                  </form>
                </FormDisclosure>
              )}
              {line.kind === "recipe" && line.fulfillment_state === "draft" && (
                <Button
                  type="submit"
                  variant="primary"
                  className={styles.button}
                  disabled={busy || !online}
                  onClick={() =>
                    void send("kitchen.send", {
                      id: line.fulfillment_id,
                      expectedVersion: line.fulfillment_version,
                    })
                  }
                >
                  {t("ส่งครัว", "Send to kitchen")}
                </Button>
              )}
            </article>
          ))}
          {!!drafts.length && (
            <section className={styles.draftCart}>
              <h3>{t("ร่างในเครื่อง", "Device draft")}</h3>
              {drafts.map((d) => (
                <div key={d.key} className={styles.row}>
                  <span>
                    {d.name} × {d.quantityMilli / 1000}
                  </span>
                  <Button
                    variant="secondary"
                    type="button"
                    className={`${styles.button} ${styles.secondary}`}
                    onClick={() => setDrafts((rows) => rows.filter((row) => row.key !== d.key))}
                  >
                    {t("ลบ", "Remove")}
                  </Button>
                </div>
              ))}
              {staleDrafts && (
                <p role="alert">
                  {t(
                    "ราคา catalog หรือเวอร์ชันบิลเปลี่ยน ตรวจรายการก่อนส่ง",
                    "Prices, catalog or bill version changed. Review before sending.",
                  )}
                </p>
              )}
              <Button
                type="submit"
                variant="secondary"
                className={`${styles.button} ${styles.secondary}`}
                disabled={!online || busy}
                onClick={() =>
                  setDrafts((rows) =>
                    rows.map((d) => {
                      const p = products.find((p) => p.id === d.productId);
                      return p
                        ? {
                            ...d,
                            expectedPriceMinor: n(p, "price_minor"),
                            expectedCatalogVersion: n(p, "catalog_version"),
                            saleId: sale ? s(sale, "id") : null,
                            saleVersion: sale ? n(sale, "version") : null,
                          }
                        : d;
                    }),
                  )
                }
              >
                {t(
                  "ตรวจแล้ว ใช้ราคาและเวอร์ชันล่าสุด",
                  "Reviewed · use current prices and version",
                )}
              </Button>
              <Button
                type="submit"
                variant="primary"
                className={styles.button}
                disabled={!online || busy || staleDrafts}
                onClick={() => void sendDrafts()}
              >
                {t("ส่งร่างเข้าบิล", "Send draft to sale")}
              </Button>
            </section>
          )}
          {sale && (
            <>
              {editable && (
                <FormDisclosure>
                  <summary>
                    {register?.mode === "restaurant"
                      ? t("ส่วนลดทั้งบิล / ย้ายโต๊ะ / รวมบิล", "Bill discount / move table / merge")
                      : t("ส่วนลดทั้งบิล / รวมบิล", "Bill discount / merge")}
                  </summary>
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      const f = form(e);
                      void command("sale.discount", { amountMinor: amount(f, "discount") });
                    }}
                  >
                    <Field>
                      {t("ส่วนลดทั้งบิลเพิ่มเติม", "Additional bill discount")}
                      <TextInput
                        name="discount"
                        type="number"
                        min="0"
                        step="0.01"
                        defaultValue="0"
                      />
                    </Field>
                    <Button
                      clientManaged type="submit"
                      variant="primary"
                      className={styles.button}
                      disabled={busy || !online}
                    >
                      {t("ใช้ส่วนลด", "Apply discount")}
                    </Button>
                  </form>
                  {register?.mode === "restaurant" && (
                    <form
                      className={styles.form}
                      onSubmit={(e) => {
                        const f = form(e);
                        void command("sale.move_table", { tableId: String(f.get("table")) });
                      }}
                    >
                      <Field>
                        {t("ย้ายโต๊ะ", "Move table")}
                        <Select name="table">
                          {data.tables
                            .filter(
                              (table) =>
                                table.register_id === registerId &&
                                !pendingSales.some((row) => row.table_id === table.id),
                            )
                            .map((table) => (
                              <option key={s(table, "id")} value={s(table, "id")}>
                                {s(table, "name")}
                              </option>
                            ))}
                        </Select>
                      </Field>
                      <Button
                        clientManaged type="submit"
                        variant="primary"
                        className={styles.button}
                        disabled={busy || !online}
                      >
                        {t("ย้าย", "Move")}
                      </Button>
                    </form>
                  )}
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      const f = form(e),
                        target = pendingSales.find((row) => row.id === f.get("target"));
                      if (target)
                        void command("sale.merge", {
                          targetId: target.id,
                          targetVersion: target.version,
                        }).then((result) => {
                          if (result) pickSale(target);
                        });
                    }}
                  >
                    <Field>
                      {t("รวมเข้าบิล", "Merge into")}
                      <Select name="target">
                        {pendingSales
                          .filter(
                            (row) =>
                              row.id !== sale.id &&
                              row.party_id === sale.party_id &&
                              row.shift_id === sale.shift_id,
                          )
                          .map((row) => (
                            <option key={s(row, "id")} value={s(row, "id")}>
                              {s(row, "number")}
                            </option>
                          ))}
                      </Select>
                    </Field>
                    <Button
                      clientManaged type="submit"
                      variant="primary"
                      className={styles.button}
                      disabled={busy || !online}
                    >
                      {t("รวมบิล", "Merge")}
                    </Button>
                  </form>
                </FormDisclosure>
              )}
              <div className={styles.row}>
                <span>{t("ส่วนลด", "Discount")}</span>
                <span>{money(n(sale, "discount_minor"))}</span>
              </div>
              <div className={styles.row}>
                <span>{t("ภาษี", "Tax")}</span>
                <span>{money(n(sale, "tax_minor"))}</span>
              </div>
              <div className={styles.total}>
                <span>{t("รวมสุทธิ", "Total")}</span>
                <strong>{money(n(sale, "total_minor"))}</strong>
              </div>
              {data.payments.map((payment) => (
                <div className={styles.paymentCard} key={s(payment, "id")}>
                  <div className={styles.row}>
                    <span>
                      {methodNames[s(payment, "method")]?.[language === "th" ? 0 : 1]} ·{" "}
                      {money(n(payment, "amount_minor"))}
                    </span>
                    <span className={styles.tag}>
                      {payment.provider_payment_confirmed && payment.state !== "succeeded"
                        ? t(
                            "Stripe ยืนยันรับเงินแล้ว · รอลงบัญชี",
                            "Stripe payment confirmed · accounting pending",
                          )
                        : s(payment, "state")}
                    </span>
                  </div>
                  {payment.method === "cash" && payment.state === "succeeded" && (
                    <p>
                      {t("เงินทอน", "Change")}{" "}
                      {money(n(payment, "tendered_minor") - n(payment, "amount_minor"))}
                    </p>
                  )}
                  <PosPromptPayEmailRecovery
                    payment={payment}
                    send={send}
                    busy={busy}
                    online={online}
                  />
                  {!payment.provider_payment_confirmed &&
                    payment.qr_data &&
                    payment.state === "pending" && <PaymentQr value={s(payment, "qr_data")} />}{" "}
                  {!payment.provider_payment_confirmed &&
                    payment.checkout_url &&
                    ["requested", "pending", "exception"].includes(s(payment, "state")) && (
                      <>
                        <PaymentQr value={s(payment, "checkout_url")} />
                        <a href={s(payment, "checkout_url")} target="_blank" rel="noreferrer">
                          {t("เปิดหน้าชำระ Stripe", "Open Stripe checkout")}
                        </a>
                      </>
                    )}
                  {["requested", "pending", "exception"].includes(s(payment, "state")) &&
                    payment.method !== "transfer" && (
                      <Button
                        type="submit"
                        variant="secondary"
                        className={`${styles.button} ${styles.secondary}`}
                        disabled={busy || !online}
                        onClick={() =>
                          void send("payment.cancel", {
                            id: payment.id,
                            expectedVersion: payment.version,
                          })
                        }
                      >
                        {payment.provider_payment_confirmed
                          ? t("ตรวจผลการลงบัญชี", "Check accounting result")
                          : t("ตรวจผล / ยกเลิกรอ", "Check result / cancel wait")}
                      </Button>
                    )}
                </div>
              ))}
              {data.payments.filter(payment=>payment.method==='transfer'&&payment.state==='pending').map(payment=><PosPendingTransfer
                key={s(payment,'id')} payment={payment} data={data} currency={s(sale,'currency')||data.currency}
                canVerify={false} canCancel={Boolean(data.permissions['pos.sell'])} send={send} busy={busy} online={online}
              />)}
              {sale.state !== "completed" && sale.state !== "cancelled" && (
                <>
                  <p className={styles.balance}>
                    {t("คงเหลือชำระ", "Amount due")} <strong>{money(due)}</strong>
                  </p>
                  {postingPending && (
                    <p role="status" className={styles.alert}>
                      {t(
                        "Stripe ยืนยันรับเงินแล้ว แต่บัญชียังไม่ครบ ให้ผู้จัดการหรือ Finance แก้รายการค้างและตรวจบิลเดิม ไม่ต้องชำระซ้ำ",
                        "Stripe confirmed payment. The manager or Finance must resolve the accounting exception and recover this bill without charging again.",
                      )}
                    </p>
                  )}
                  {waiting > 0 && !postingPending && (
                    <p role="status">
                      {t(
                        "รอการยืนยันรับเงิน ห้ามใช้หน้ากลับจาก Stripe เป็นหลักฐานสำเร็จ",
                        "Waiting for verified payment; a Stripe return is not payment confirmation.",
                      )}
                    </p>
                  )}
                  {due - waiting > 0 && (
                    <form
                      key={`${sale.id}:${due}:${waiting}`}
                      className={styles.paymentForm}
                      onSubmit={(e) => {
                        const f = form(e);
                        void command("payment.create", {
                          method,
                          amountMinor: amount(f, "amount"),
                          tenderedMinor:
                            method === "cash" && f.get("tender")
                              ? amount(f, "tender")
                              : amount(f, "amount"),
                          reference: String(f.get("reference") ?? ""),
                          payerEmail: String(f.get("payerEmail") ?? ""),
                        });
                      }}
                    >
                      <Field>
                        {t("วิธีรับเงิน", "Payment method")}
                        <Select value={method} onChange={(e) => setMethod(e.target.value)}>
                          {data.availableTenderMethods.map((m) => (
                            <option key={m} value={m}>
                              {methodNames[m]?.[language === "th" ? 0 : 1]}
                            </option>
                          ))}
                        </Select>
                      </Field>
                      <Field>
                        {t("จำนวนเงิน · แบ่งชำระได้", "Amount · split tender")}
                        <TextInput
                          name="amount"
                          type="number"
                          min="0.01"
                          step="0.01"
                          max={(due - waiting) / 100}
                          defaultValue={(due - waiting) / 100}
                          required
                        />
                      </Field>
                      {method === "cash" && (
                        <Field>
                          {t("เงินสดที่รับ", "Cash tendered")}
                          <TextInput name="tender" type="number" min="0" step="0.01" />
                        </Field>
                      )}
                      {["transfer", "edc"].includes(method) && (
                        <Field>
                          {t("เลขอ้างอิง", "Reference")}
                          <TextInput name="reference" required maxLength={150} />
                        </Field>
                      )}
                      {method === "stripe_promptpay" && (
                        <Field>
                          {t(
                            "อีเมลผู้จ่ายและคืนเงิน PromptPay",
                            "Email for PromptPay payment and refund",
                          )}
                          <TextInput name="payerEmail" type="email" required />
                        </Field>
                      )}
                      <Button
                        clientManaged type="submit"
                        variant="primary"
                        className={styles.button}
                        disabled={busy || !online || !!drafts.length}
                      >
                        {t("รับชำระเงิน", "Record payment")}
                      </Button>
                    </form>
                  )}
                  <div className={styles.actions}>
                    <Button
                      type="submit"
                      variant="primary"
                      className={styles.button}
                      disabled={
                        busy ||
                        !online ||
                        due > 0 ||
                        waiting > 0 ||
                        !data.lines.length ||
                        !!drafts.length
                      }
                      onClick={() => void command("sale.complete")}
                    >
                      {t("ยืนยันและปิดบิล", "Complete sale")}
                    </Button>
                    {editable && (
                      <>
                        <Button
                          type="submit"
                          variant="secondary"
                          className={`${styles.button} ${styles.secondary}`}
                          disabled={busy || !online}
                          onClick={() =>
                            void command(sale.state === "held" ? "sale.resume" : "sale.hold")
                          }
                        >
                          {sale.state === "held"
                            ? t("เรียกบิล", "Resume")
                            : t("พักบิล", "Hold sale")}
                        </Button>
                        <Button
                          type="submit"
                          variant="secondary"
                          className={`${styles.button} ${styles.secondary}`}
                          disabled={busy || !online}
                          onClick={() => void command("sale.cancel")}
                        >
                          {t("ยกเลิกบิล", "Cancel sale")}
                        </Button>
                      </>
                    )}
                  </div>
                  {data.roomChargeEnabled && editable && data.roomChargeStays.length > 0 && (
                    <details>
                      <summary>{t("ลงบัญชีห้องพัก", "Charge to room")}</summary>
                      <form
                        className={styles.form}
                        onSubmit={(e) => {
                          const f = form(e),
                            stay = data.roomChargeStays.find(
                              (row) => row.stay_id === f.get("stay"),
                            );
                          if (stay)
                            void command("sale.charge_to_room", {
                              propertyId: stay.property_id,
                              stayId: stay.stay_id,
                              expectedPropertyVersion: stay.property_version,
                            });
                        }}
                      >
                        <Field>
                          {t("ตรวจห้องและชื่อผู้เข้าพัก", "Verify room and guest")}
                          <Select name="stay" required>
                            <option value="">{t("เลือกผู้เข้าพัก", "Choose guest")}</option>
                            {data.roomChargeStays.map((row) => (
                              <option key={s(row, "stay_id")} value={s(row, "stay_id")}>
                                {s(row, "room_number")} · {s(row, "guest_name")}
                              </option>
                            ))}
                          </Select>
                        </Field>
                        <Button
                          clientManaged type="submit"
                          variant="primary"
                          className={styles.button}
                          disabled={busy || !online || !data.lines.length}
                        >
                          {t("ลงบิลเต็มเข้าห้อง", "Charge full bill")}
                        </Button>
                      </form>
                    </details>
                  )}
                </>
              )}
              {data.roomCharge && (
                <p role="status" className={styles.alert}>
                  {t("ลงบัญชีห้องพักแล้ว", "Charged to hotel folio")}
                </p>
              )}
              {sale.state === "completed" && data.receipt && (
                <Button
                  type="submit"
                  variant="primary"
                  className={styles.button}
                  onClick={() => window.print()}
                >
                  {t("พิมพ์ใบเสร็จ / พิมพ์ซ้ำ", "Print / reprint receipt")}
                </Button>
              )}
              {sale.state==='completed'&&data.taxDocumentInvoiceId&&<>
                <a href={`${erpBase}/finance/invoices/${data.taxDocumentInvoiceId}`}>{t('เอกสารบัญชี','Accounting document')}</a>
                <TaxDocumentReference base={erpBase} invoiceId={data.taxDocumentInvoiceId} endpoint={`${api}/tax-documents`}/>
              </>}
              {data.refunds.some((row) => row.sale_id === sale.id) && (
                <section aria-label={t("สถานะการคืนเงิน", "Refund status")} aria-live="polite">
                  <h3>{t("สถานะการคืนเงิน", "Refund status")}</h3>
                  {data.refunds
                    .filter((row) => row.sale_id === sale.id)
                    .map((row) => (
                      <article className={styles.paymentCard} key={s(row, "id")}>
                        <strong>
                          {money(n(row, "amount_minor"))} ·{" "}
                          {(
                            {
                              requested: t("รอผู้อนุมัติอีกคนตรวจ", "Awaiting independent review"),
                              approved: t("อนุมัติแล้ว · รอคืนเงิน", "Approved · awaiting payout"),
                              processing: t("กำลังคืนเงิน", "Refund processing"),
                              succeeded: t("คืนเงินสำเร็จ", "Refund completed"),
                              completed: t("คืนเงินสำเร็จ", "Refund completed"),
                              rejected: t("คำขอถูกปฏิเสธ", "Request rejected"),
                              failed: t("คืนเงินไม่สำเร็จ", "Refund failed"),
                              exception: t("ต้องตรวจสอบ", "Needs attention"),
                            } as Record<string, string>
                          )[s(row, "state")] ?? s(row, "state")}
                        </strong>
                        <p>{s(row, "reason")}</p>
                      </article>
                    ))}
                </section>
              )}
              {["completed", "payment_pending"].includes(s(sale, "state")) && !data.roomCharge && (
                <FormDisclosure className={styles.refundSection}>
                  <summary>{t("คืนบางรายการ / คืนเงิน", "Partial return / refund")}</summary>
                  <form
                    className={styles.fields}
                    onSubmit={(e) => {
                      const target = e.currentTarget,
                        f = form(e),
                        lines = data.lines
                          .filter((l) => Number(f.get(`qty:${l.id}`)) > 0)
                          .map((l) => ({
                            lineId: l.id,
                            quantityMilli: quantity(f, `qty:${l.id}`),
                            restock: f.get(`restock:${l.id}`) === "on",
                          }));
                      void command("refund.request", {
                        lines: sale.state === "completed" ? lines : [],
                        amountMinor: sale.state === "completed" ? undefined : amount(f, "advance"),
                        reason: String(f.get("reason")),
                        payoutShiftId: String(f.get("shift") ?? "") || undefined,
                      }).then((result) => {
                        if (result) target.reset();
                      });
                    }}
                  >
                    {sale.state === "completed" ? (
                      data.lines
                        .filter((l) => n(l, "quantity_milli") > n(l, "returned_milli"))
                        .map((l) => (
                          <div key={s(l, "id")}>
                            <Field>
                              {s(l, "name")}
                              <TextInput
                                name={`qty:${l.id}`}
                                type="number"
                                min="0"
                                max={(n(l, "quantity_milli") - n(l, "returned_milli")) / 1000}
                                step="0.001"
                                defaultValue="0"
                              />
                            </Field>
                            {l.kind === "stock" && (
                              <Field orientation="inline" className={styles.check}>
                                <TextInput type="checkbox" name={`restock:${l.id}`} />
                                {t("สินค้าขายต่อได้ · คืนสต็อก", "Resalable · restock")}
                              </Field>
                            )}
                          </div>
                        ))
                    ) : (
                      <Field>
                        {t("คืนเงินรับล่วงหน้า", "Advance refund")}
                        <TextInput name="advance" type="number" min="0.01" step="0.01" required />
                      </Field>
                    )}
                    <Field>
                      {t("กะจ่ายเงินสดคืน", "Cash payout session")}
                      <Select name="shift">
                        <option value="">
                          {t("เลือกเมื่อคืนเงินสด", "Choose for cash refund")}
                        </option>
                        {data.shifts
                          .filter((row) => row.state === "open")
                          .map((row) => (
                            <option key={s(row, "id")} value={s(row, "id")}>
                              {s(row, "register_name")}
                            </option>
                          ))}
                      </Select>
                    </Field>
                    <Field>
                      {t("เหตุผล", "Reason")}
                      <TextInput name="reason" minLength={3} required />
                    </Field>
                    <Button
                      clientManaged type="submit"
                      variant="primary"
                      className={styles.button}
                      disabled={busy || !online}
                    >
                      {t("ส่งให้ผู้อนุมัติอีกคน", "Request independent approval")}
                    </Button>
                  </form>
                </FormDisclosure>
              )}
            </>
          )}
        </aside>
      </div>
      <div className={styles.mobileSaleActions} aria-label={t("ทางลัดการขาย", "Sale shortcuts")}>
        <Button
          variant="plain"
          type="button"
          onClick={() => catalogRef.current?.scrollIntoView({ block: "start" })}
        >
          {t("เลือกสินค้า", "Products")}
        </Button>
        <Button
          variant="plain"
          type="button"
          onClick={() => cartRef.current?.scrollIntoView({ block: "start" })}
        >
          <span>
            {t("ตะกร้า", "Cart")} · {data.lines.length + drafts.length}
          </span>
          <strong>{money(sale ? n(sale, "total_minor") : 0)}</strong>
        </Button>
      </div>
      <PosReceipt receipt={data.receipt} />
    </>
  );
}
