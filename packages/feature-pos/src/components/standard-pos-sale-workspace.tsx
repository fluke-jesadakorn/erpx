"use client";
import type {PosWorkspaceCompletion} from "@erpx/feature-pos/lib/workspace-command";
import WorkspaceLink from '@erpx/workplace/components/workspace-link';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import dynamic from "next/dynamic";
const RoomChargeReversalRequest = dynamic(() => import("@erpx/feature-shared/components/hospitality/room-charge-reversals").then(module => module.RoomChargeReversalRequest));
const TaxDocumentReference = dynamic(() => import("@erpx/feature-shared/components/hospitality/tax-document-reference").then(module => module.TaxDocumentReference));
import {PosPendingTransfer} from "@erpx/feature-pos/components/pos-pending-transfer";
import {PosReceipt} from "@erpx/feature-pos/components/pos-receipt";

import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { Button, Card, Field, Select, TextInput } from "@erpx/ui/components/foundation";

import { useEffect, useLayoutEffect, useRef, useState, type FormEvent } from "react";
import type { PosWorkspace } from "@erpx/server-pos/public/pos";
import type { PosRow } from "@erpx/server-pos/public/pos-core";

import { formatPosMoney } from "@erpx/server-pos/public/rules/pos";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { useFrontLanguage } from "@erpx/front/components/front-language";
import QRCode from "qrcode";
import Image from "next/image";
import geometry from "@erpx/feature-pos/components/pos.module.css";
import presentation from "@erpx/feature-pos/components/standard-pos-workflow.module.css";
const styles={...geometry,...presentation};
import {readPosDeviceDrafts,writePosDeviceDrafts,retirePosDeviceDraft,subscribePosDeviceDrafts,subscribePosDevicePending,type PosDeviceDraft,type PosDeviceDraftReference} from '@erpx/feature-pos/lib/device-cart-command';
import {SourceSupportLink} from "@erpx/feature-support/components/source-support-link";

type Row = PosRow;
const s = (row: Row, key: string) => String(row[key] ?? "");
const n = (row: Row, key: string) => Number(row[key] ?? 0);
const form = (e: FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  return new FormData(e.currentTarget);
};
const amount = (f: FormData, key: string) => Math.round(Number(f.get(key) ?? 0) * 100);
const quantity = (f: FormData, key: string) => Math.round(Number(f.get(key) ?? 0) * 1000);

export type PosSend = (
  op: string,
  input: unknown,
  select?: boolean,
  deviceDraft?: PosDeviceDraftReference,
  completion?: PosWorkspaceCompletion,
) => Promise<Record<string, unknown> | undefined>;
const methodNames: Record<string, [string, string]> = {
  cash: ["เงินสด", "Cash"],
  transfer: ["โอนธนาคาร · รอ Finance ตรวจ", "Bank transfer · Finance verification"],
  edc: ["เครื่องรูดบัตรภายนอก", "External EDC"],
  stripe_card: ["บัตร · Stripe", "Card · Stripe"],
  stripe_promptpay: ["PromptPay · Stripe", "PromptPay · Stripe"],
};

/** Both sale screens recover the same unverified transfer through its owner command. */


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
export function StandardPosPromptPayEmailRecovery({
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
      <Button appearance="standard"
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
      <Field appearance="standard">
        {t("อีเมลผู้จ่าย PromptPay", "PromptPay payer email")}
        <TextInput appearance="standard" name="payerEmail" type="email" required />
      </Field>
      <Button appearance="standard" clientManaged type="submit" variant="primary" className={styles.button} disabled={busy || !online}>
        {t("บันทึกอีเมลและสร้าง QR รายการเดิม", "Save email and retry existing QR")}
      </Button>
    </form>
  );
}



type Draft = PosDeviceDraft;
export function StandardPosSaleWorkspace({
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
  const cartSaveError=t('บันทึกร่างเพื่อกู้คืนไม่ได้ กรุณาเก็บร่างเดิมและลองอีกครั้ง','The device cart could not be saved for recovery. Keep it and retry.');
  const draftSave = useRef<{storageKey:string;drafts:Draft[]}|null>(null);
  useLayoutEffect(()=>{
    const captured={storageKey,drafts};draftSave.current=captured;
    return()=>{if(draftSave.current===captured)draftSave.current=null;};
  },[storageKey,drafts]);
  useEffect(() => {
    let live = true;
    queueMicrotask(() => {
      if (!live) return;
      try {
        setDrafts(readPosDeviceDrafts(localStorage,storageKey));
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
    if(loadedDraftKey!==storageKey)return;
    let live=true;
    const captured=draftSave.current;if(!captured)return;
    const isCurrent=()=>live&&draftSave.current===captured;
    const assertCurrent=()=>{if(!isCurrent())throw Error('This device cart save was superseded.');};
    void writePosDeviceDrafts(localStorage,storageKey,drafts,assertCurrent).then(saved=>{
      if(isCurrent()&&JSON.stringify(saved)!==JSON.stringify(drafts))setDrafts(current=>isCurrent()&&current===drafts?saved:current);
    }).catch(()=>{if(isCurrent())setLocalError(cartSaveError);});
    return()=>{live=false;};
  }, [loadedDraftKey, drafts, storageKey, cartSaveError]);
  useEffect(() => subscribePosDevicePending(storageKey,draft => {
    setDrafts(rows => rows.some(item=>item.key===draft.key)?rows.map(item=>item.key===draft.key?{...item,submission:draft.submission}:item):[...rows,draft]);
  }), [storageKey]);
  useEffect(() => subscribePosDeviceDrafts(storageKey,ack => {
    setDrafts(rows => retirePosDeviceDraft(rows,ack));
  }), [storageKey]);
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
  const command = (op: string, extra: Record<string, unknown> = {}, completion?:PosWorkspaceCompletion) =>
    sale
      ? send(op, { id: sale.id, expectedVersion: sale.version, ...extra }, false, undefined, completion)
      : Promise.resolve(undefined);
  async function createSale(tableId: string | null = null, selectCreated=true, completion?:PosWorkspaceCompletion) {
    if (!shift || data.purchases && (!data.purchases["pos.sales"].newWork || register?.mode === "restaurant" && !data.purchases["pos.restaurant"].newWork)) return undefined;
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
      selectCreated,
      undefined,
      completion,
    );
  }
  const latestChosen=useRef(chosen);useLayoutEffect(()=>{latestChosen.current=chosen;},[chosen]);
  const latestSearch=useRef(search);useLayoutEffect(()=>{latestSearch.current=search;},[search]);
  async function addProduct(
    product: Row,
    input: { quantityMilli: number; modifierIds: string[]; discountMinor: number; note: string },
    submittedForm?:HTMLFormElement,
    completion?:{current():boolean;acknowledged():void},
  ) {
    const submittedFields=submittedForm?JSON.stringify(Array.from(new FormData(submittedForm))):null;
    const unchanged=()=>(completion?completion.current():latestChosen.current===product)&&(!submittedForm||submittedForm.isConnected&&JSON.stringify(Array.from(new FormData(submittedForm)))===submittedFields);
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
      if(unchanged())completion?.acknowledged();
      return;
    }
    const submit=(saleId:unknown,version:unknown)=>send(
      "sale.add",
      {
        id:saleId,
        expectedVersion:version,
        ...input,
        productId: product.id,
        expectedPriceMinor: draft.expectedPriceMinor,
        expectedCatalogVersion: draft.expectedCatalogVersion,
      },
      true,
      undefined,
      {form:submittedForm,current:unchanged,acknowledged:()=>{setChosen(null);completion?.acknowledged();}},
    );
    if (!sale) {
      // A lost constructor acknowledgement retains this exact dependent add.
      // Its known native id is adopted even if newer fields prevent the add;
      // the next deliberate add therefore uses that sale, not a new constructor.
      await createSale(null,false,{form:submittedForm,current:unchanged,notifySaved:false,acknowledged:created=>()=>submit(created.id,1)});
      return;
    }
    await submit(sale.id,sale.version);
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
    if (!online) return;
    // A stored first submission owns its original body/version even if today's
    // catalogue or selected bill has changed. Never turn it into a fresh add.
    const original=drafts.find(draft=>draft.submission);
    if(original?.submission){
      await send('sale.add',JSON.parse(original.submission.body),true,{storageKey,key:original.key});return;
    }
    if(staleDrafts)return;
    let queued:Draft[];try{queued=await writePosDeviceDrafts(localStorage,storageKey,drafts);}catch{setLocalError(t('บันทึกร่างเพื่อกู้คืนไม่ได้ กรุณาเก็บร่างเดิมและลองอีกครั้ง','The device cart could not be saved for recovery. Keep it and retry.'));return;}
    let saleId = sale?.id,
      version = sale?.version;
    const requested = new Map<string, number>();
    for (const draft of queued)
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
    for (const draft of queued) {
      const { key: _key, name: _name, saleId: _sale, saleVersion: _version, submission: _submission, imageDigest: _digest, ...input } = draft;
      void _key;
      void _name;
      void _sale;
      void _version;
      void _submission;
      void _digest;
      const result = await send(
        "sale.add",
        { id: saleId, expectedVersion: version, ...input },
        true,
        {storageKey,key:draft.key},
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
        <Field appearance="standard">
          {t("จุดขาย", "Register")}
          <Select appearance="standard"
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
          <Button appearance="standard" type="submit" variant="primary" className={styles.button} onClick={openShifts}>
            {t("เปิดกะก่อนขาย", "Open cash shift")}
          </Button>
        )}
        <Button appearance="standard"
          type="submit"
          variant="secondary"
          className={`${styles.button} ${styles.secondary}`}
          disabled={busy || !online || !shift || postingPending || data.purchases && (!data.purchases["pos.sales"].newWork || register?.mode === "restaurant" && !data.purchases["pos.restaurant"].newWork)}
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
                      <Button appearance="standard"
                        type="submit"
                        variant="plain"
                        className={`${styles.tableTile} ${occupied ? styles.occupied : ""}`}
                        key={s(table, "id")}
                        disabled={busy || !online || !shift || !occupied && !!data.purchases && (!data.purchases["pos.sales"].newWork || !data.purchases["pos.restaurant"].newWork)}
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
                  <Button appearance="standard"
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
          <Card appearance="standard" className={styles.panel}>
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
                if (product) {
                  const submittedSearch=search;
                  void addProduct(product, {
                    quantityMilli: 1000,
                    modifierIds: [],
                    discountMinor: 0,
                    note: "",
                  },e.currentTarget,{current:()=>latestSearch.current===submittedSearch,acknowledged:()=>setSearch("")});
                }
              }}
            >
              <TextInput appearance="standard"
                autoComplete="off"
                aria-label={t("ค้นหา / สแกนบาร์โค้ด", "Search / scan barcode")}
                placeholder={t(
                  "ค้นหาชื่อสินค้า SKU หรือสแกนบาร์โค้ด",
                  "Search name, SKU or scan barcode",
                )}
                value={search}
                disabled={data.purchases && !data.purchases["pos.sales"].newWork}
                onChange={(e) => setSearch(e.target.value)}
              />
              <Button appearance="standard"
                clientManaged type="submit"
                variant="primary"
                className={styles.button}
                disabled={busy || (!shift && online) || data.purchases && !data.purchases["pos.sales"].newWork}
              >
                {t("เพิ่ม", "Add")}
              </Button>
            </form>
            <div className={styles.categoryBar}>
              <Button appearance="standard"
                type="submit"
                variant="plain"
                className={styles.category}
                aria-pressed={!category}
                onClick={() => setCategory("")}
              >
                {t("ทั้งหมด", "All")}
              </Button>
              {[...new Set(products.map((p) => s(p, "category")).filter(Boolean))].map((c) => (
                <Button appearance="standard"
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
                  }, e.currentTarget);
                }}
              >
                <div className={styles.header}>
                  <h3>{s(chosen, "name")}</h3>
                  <Button appearance="standard"
                    variant="secondary"
                    type="button"
                    className={`${styles.button} ${styles.secondary}`}
                    onClick={() => setChosen(null)}
                  >
                    {t("ปิด", "Close")}
                  </Button>
                </div>
                <div className={styles.fields}>
                  <Field appearance="standard">
                    {t("จำนวน", "Quantity")}
                    <TextInput appearance="standard"
                      name="quantity"
                      type="number"
                      step="0.001"
                      min="0.001"
                      defaultValue="1"
                      required
                    />
                  </Field>
                  <Field appearance="standard">
                    {t("ส่วนลดรายการ", "Line discount")}
                    <TextInput appearance="standard" name="discount" type="number" step="0.01" min="0" defaultValue="0" />
                  </Field>
                  <Field appearance="standard">
                    {t("หมายเหตุ", "Notes")}
                    <TextInput appearance="standard" name="note" maxLength={500} />
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
                    <Field appearance="standard" orientation="inline" key={m.id}>
                      <TextInput appearance="standard" type="checkbox" name="modifier" value={m.id} />
                      {m.name} +{money(m.priceMinor)}
                    </Field>
                  ))}
                </div>
                <Button appearance="standard"
                  clientManaged type="submit"
                  variant="primary"
                  className={styles.button}
                  disabled={busy || (!!sale && !editable) || data.purchases && (!data.purchases["pos.sales"].newWork || chosen?.kind === "recipe" && !data.purchases["pos.restaurant"].newWork)}
                >
                  {t("เพิ่มเข้าตะกร้า", "Add to cart")}
                </Button>
              </form>
            )}
            <div className={styles.products}>
              {visible.map((product) => (
                <Button appearance="standard"
                  variant="plain"
                  type="button"
                  className={styles.product}
                  key={s(product, "id")}
                  disabled={
                    busy ||
                    !!data.purchases && (!data.purchases["pos.sales"].newWork || product.kind === "recipe" && !data.purchases["pos.restaurant"].newWork) ||
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
          </Card>
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
          <FormDisclosure appearance="standard" className={styles.customerPicker}>
            <summary>
              {t("ลูกค้า", "Customer")} ·{" "}
              {s(customers.find((c) => c.id === (sale?.party_id ?? customerId)) ?? {}, "name") ||
                t("ลูกค้าทั่วไป", "Walk-in")}
            </summary>
            <TextInput appearance="standard"
              aria-label={t("ค้นหาลูกค้า CRM", "Search CRM customers")}
              placeholder={t("ชื่อ เบอร์โทร หรืออีเมล", "Name, phone or email")}
              value={customerQuery}
              onChange={(e) => setCustomerQuery(e.target.value)}
            />
            <Select appearance="standard"
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
            <Button appearance="standard"
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
                  }, false, undefined, {form:e.currentTarget,acknowledged:result=>{
                    const customer = { id: String(result.id), name: String(result.name) };
                    setCustomers((rows) => [customer, ...rows]);
                    setCustomerId(customer.id);
                    setNewCustomer(false);
                    // A retained first constructor completion can continue its
                    // original dependent selection after its receipt is saved.
                    return sale?()=>command("sale.customer", { partyId: customer.id }):undefined;
                  }});
                }}
              >
                <Field appearance="standard">
                  {t("ชื่อ", "Name")}
                  <TextInput appearance="standard" name="name" required maxLength={200} />
                </Field>
                <Field appearance="standard">
                  {t("โทรศัพท์", "Phone")}
                  <TextInput appearance="standard" name="phone" type="tel" maxLength={50} />
                </Field>
                <Field appearance="standard">
                  {t("อีเมล", "Email")}
                  <TextInput appearance="standard" name="email" type="email" maxLength={320} />
                </Field>
                <Field appearance="standard">
                  {t("ที่อยู่", "Address")}
                  <TextInput appearance="standard" name="address" maxLength={200} />
                </Field>
                <Field appearance="standard">
                  {t("จังหวัด / เมือง", "City")}
                  <TextInput appearance="standard" name="city" maxLength={100} />
                </Field>
                <Field appearance="standard">
                  {t("รหัสไปรษณีย์", "Postal code")}
                  <TextInput appearance="standard" name="postal" maxLength={30} />
                </Field>
                <Button appearance="standard"
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
                <FormDisclosure appearance="standard">
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
                    <Field appearance="standard">
                      {t("จำนวน", "Quantity")}
                      <TextInput appearance="standard"
                        name="quantity"
                        type="number"
                        step="0.001"
                        min="0.001"
                        defaultValue={n(line, "quantity_milli") / 1000}
                      />
                    </Field>
                    <Field appearance="standard">
                      {t("ส่วนลดรายการ", "Line discount")}
                      <TextInput appearance="standard"
                        name="discount"
                        type="number"
                        step="0.01"
                        min="0"
                        defaultValue={
                          (n(line, "discount_minor") - n(line, "bill_discount_minor")) / 100
                        }
                      />
                    </Field>
                    <Field appearance="standard">
                      {t("หมายเหตุ", "Notes")}
                      <TextInput appearance="standard" name="note" defaultValue={s(line, "note")} />
                    </Field>
                    <Button appearance="standard"
                      clientManaged type="submit"
                      variant="primary"
                      className={styles.button}
                      disabled={busy || !online}
                    >
                      {t("บันทึก", "Save")}
                    </Button>
                    <Button appearance="standard"
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
                      }, {form:e.currentTarget,acknowledged:result=>()=>pickSale({id:String(result.id),register_id:registerId})});
                    }}
                  >
                    <Field appearance="standard">
                      {t("จำนวนที่แยก", "Quantity to split")}
                      <TextInput appearance="standard"
                        name="quantity"
                        type="number"
                        step="0.001"
                        min="0.001"
                        max={n(line, "quantity_milli") / 1000}
                        defaultValue={n(line, "quantity_milli") / 1000}
                      />
                    </Field>
                    <Button appearance="standard"
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
                <Button appearance="standard"
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
                  <Button appearance="standard"
                    variant="secondary"
                    type="button"
                    className={`${styles.button} ${styles.secondary}`}
                    disabled={Boolean(d.submission)}
                    onClick={() => setDrafts((rows) => rows.filter((row) => row.key !== d.key || Boolean(row.submission)))}
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
              <Button appearance="standard"
                type="submit"
                variant="secondary"
                className={`${styles.button} ${styles.secondary}`}
                disabled={!online || busy || drafts.some(draft=>draft.submission)}
                onClick={() =>
                  setDrafts((rows) =>
                    rows.map((d) => {
                      if(d.submission)return d;
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
              <Button appearance="standard"
                type="submit"
                variant="primary"
                className={styles.button}
                disabled={!online || busy || staleDrafts && !drafts.some(draft=>draft.submission)}
                onClick={() => void sendDrafts()}
              >
                {drafts.some(draft=>draft.submission)?t("ตรวจผลร่างเดิม", "Recover original draft"):t("ส่งร่างเข้าบิล", "Send draft to sale")}
              </Button>
            </section>
          )}
          {sale && (
            <>
              {editable && (
                <FormDisclosure appearance="standard">
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
                    <Field appearance="standard">
                      {t("ส่วนลดทั้งบิลเพิ่มเติม", "Additional bill discount")}
                      <TextInput appearance="standard"
                        name="discount"
                        type="number"
                        min="0"
                        step="0.01"
                        defaultValue="0"
                      />
                    </Field>
                    <Button appearance="standard"
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
                      <Field appearance="standard">
                        {t("ย้ายโต๊ะ", "Move table")}
                        <Select appearance="standard" name="table">
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
                      <Button appearance="standard"
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
                        }, {form:e.currentTarget,acknowledged:()=>()=>pickSale(target)});
                    }}
                  >
                    <Field appearance="standard">
                      {t("รวมเข้าบิล", "Merge into")}
                      <Select appearance="standard" name="target">
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
                    <Button appearance="standard"
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
                  <StandardPosPromptPayEmailRecovery
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
                      <Button appearance="standard"
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
                        "รอการยืนยันรับเงิน ปิดการขายได้เมื่อการชำระเงินได้รับการยืนยันแล้ว",
                        "Waiting for verified payment. Complete the sale when payment is confirmed.",
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
                      <Field appearance="standard">
                        {t("วิธีรับเงิน", "Payment method")}
                        <Select appearance="standard" value={method} onChange={(e) => setMethod(e.target.value)}>
                          {data.availableTenderMethods.map((m) => (
                            <option key={m} value={m}>
                              {methodNames[m]?.[language === "th" ? 0 : 1]}
                            </option>
                          ))}
                        </Select>
                      </Field>
                      <Field appearance="standard">
                        {t("จำนวนเงิน · แบ่งชำระได้", "Amount · split tender")}
                        <TextInput appearance="standard"
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
                        <Field appearance="standard">
                          {t("เงินสดที่รับ", "Cash tendered")}
                          <TextInput appearance="standard" name="tender" type="number" min="0" step="0.01" />
                        </Field>
                      )}
                      {["transfer", "edc"].includes(method) && (
                        <Field appearance="standard">
                          {t("เลขอ้างอิง", "Reference")}
                          <TextInput appearance="standard" name="reference" required maxLength={150} />
                        </Field>
                      )}
                      {method === "stripe_promptpay" && (
                        <Field appearance="standard">
                          {t(
                            "อีเมลผู้จ่ายและคืนเงิน PromptPay",
                            "Email for PromptPay payment and refund",
                          )}
                          <TextInput appearance="standard" name="payerEmail" type="email" required />
                        </Field>
                      )}
                      <Button appearance="standard"
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
                    <Button appearance="standard"
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
                        <Button appearance="standard"
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
                        <Button appearance="standard"
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
                        <Field appearance="standard">
                          {t("ตรวจห้องและชื่อผู้เข้าพัก", "Verify room and guest")}
                          <Select appearance="standard" name="stay" required>
                            <option value="">{t("เลือกผู้เข้าพัก", "Choose guest")}</option>
                            {data.roomChargeStays.map((row) => (
                              <option key={s(row, "stay_id")} value={s(row, "stay_id")}>
                                {s(row, "room_number")} · {s(row, "guest_name")}
                              </option>
                            ))}
                          </Select>
                        </Field>
                        <Button appearance="standard"
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
              <RoomChargeReversalRequest data={data} send={send} busy={busy}/>
              {data.roomCharge && (
                <p role="status" className={styles.alert}>
                  {t("ลงบัญชีห้องพักแล้ว", "Charged to hotel guest account")}
                </p>
              )}
              {sale.state === "completed" && data.receipt && (
                <Button appearance="standard"
                  type="submit"
                  variant="primary"
                  className={styles.button}
                  onClick={() => window.print()}
                >
                  {t("พิมพ์ใบเสร็จ / พิมพ์ซ้ำ", "Print / reprint receipt")}
                </Button>
              )}
              {sale.state==='completed'&&data.taxDocumentInvoiceId&&<>
                <WorkspaceLink href={canonicalUiHref(`${erpBase}/finance/invoices/${data.taxDocumentInvoiceId}`)}>{t('เอกสารบัญชี','Accounting document')}</WorkspaceLink>
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
                <FormDisclosure appearance="standard" className={styles.refundSection}>
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
                      }, {form:target,acknowledged:()=>{target.reset();}});
                    }}
                  >
                    {sale.state === "completed" ? (
                      data.lines
                        .filter((l) => n(l, "quantity_milli") > n(l, "returned_milli"))
                        .map((l) => (
                          <div key={s(l, "id")}>
                            <Field appearance="standard">
                              {s(l, "name")}
                              <TextInput appearance="standard"
                                name={`qty:${l.id}`}
                                type="number"
                                min="0"
                                max={(n(l, "quantity_milli") - n(l, "returned_milli")) / 1000}
                                step="0.001"
                                defaultValue="0"
                              />
                            </Field>
                            {l.kind === "stock" && (
                              <Field appearance="standard" orientation="inline" className={styles.check}>
                                <TextInput appearance="standard" type="checkbox" name={`restock:${l.id}`} />
                                {t("สินค้าขายต่อได้ · คืนสต็อก", "Resalable · restock")}
                              </Field>
                            )}
                          </div>
                        ))
                    ) : (
                      <Field appearance="standard">
                        {t("คืนเงินรับล่วงหน้า", "Advance refund")}
                        <TextInput appearance="standard" name="advance" type="number" min="0.01" step="0.01" required />
                      </Field>
                    )}
                    <Field appearance="standard">
                      {t("กะจ่ายเงินสดคืน", "Cash payout session")}
                      <Select appearance="standard" name="shift">
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
                    <Field appearance="standard">
                      {t("เหตุผล", "Reason")}
                      <TextInput appearance="standard" name="reason" minLength={3} required />
                    </Field>
                    <Button appearance="standard"
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
        <Button appearance="standard"
          variant="plain"
          type="button"
          onClick={() => catalogRef.current?.scrollIntoView({ block: "start" })}
        >
          {t("เลือกสินค้า", "Products")}
        </Button>
        <Button appearance="standard"
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

export {PosReceipt} from "@erpx/feature-pos/components/pos-receipt";
export {PosPendingTransfer} from "@erpx/feature-pos/components/pos-pending-transfer";
