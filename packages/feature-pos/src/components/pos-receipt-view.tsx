import type { PosRow } from "@erpx/server-pos/public/pos-core";
import type { PosReceiptSnapshot } from "@erpx/server-pos/public/pos-daily";
import { formatPosMoney } from "@erpx/server-pos/public/rules/pos";
import styles from "@erpx/feature-pos/components/pos.module.css";
type Row = PosRow;

const s = (row: Row, key: string) => String(row[key] ?? "");

const n = (row: Row, key: string) => Number(row[key] ?? 0);

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

const methodNames: Record<string, [string, string]> = {
  cash: ["เงินสด", "Cash"],
  transfer: ["โอนธนาคาร · รอ Finance ตรวจ", "Bank transfer · Finance verification"],
  edc: ["เครื่องรูดบัตรภายนอก", "External EDC"],
  stripe_card: ["บัตร · Stripe", "Card · Stripe"],
  stripe_promptpay: ["PromptPay · Stripe", "PromptPay · Stripe"],
};

export function PosReceiptView({ receipt, language = "en" }: { receipt: PosReceiptSnapshot | null; language?: "th" | "en" }) {
  const t = (th: string, en: string) => (language === "th" ? th : en);
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
            {t("ลงบัญชีห้องพัก · ชำระตอนเช็กเอาต์", "Charged to hotel guest account · Pay at checkout")}
          </p>
        )}
        <p>{t("เอกสารนี้ไม่ใช่ใบกำกับภาษี", "This document is not a tax invoice")}</p>
      </article>
    </div>
  );
}
