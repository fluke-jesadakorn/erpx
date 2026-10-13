"use client";
import { Button, Field, Select, Table, TableFrame, TextInput } from "@/components/ui/foundation";

import { useEffect, useRef, useState, type FormEvent } from "react";
import Link from "next/link";
import { businessDate, formatPosMoney } from "@/server/modules/pos/public/rules/pos";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { createLatestRead } from "@/lib/shared/latest-read";
import { useFrontLanguage } from "@/components/front/front-language";
import type { PosWorkspace } from "@/server/modules/pos/public/pos";
import type { posDayReport } from "@/server/modules/pos/public/pos-daily";
import type { PosSend } from "./pos-sales";
import styles from "./pos.module.css";
type Report = Awaited<ReturnType<typeof posDayReport>>;

export function CashDenominationCount({
  fieldName = "counted",
  currency = "THB",
  expectedMinor,
}: {
  fieldName?: string;
  currency?: string;
  expectedMinor?: number;
}) {
  const language = useFrontLanguage(),
    t = (th: string, en: string) => (language === "th" ? th : en);
  const [counts, setCounts] = useState<Record<string, number>>({}),
    [manual, setManual] = useState(currency !== "THB"),
    [manualValue, setManualValue] = useState("");
  const denominations = [100000, 50000, 10000, 5000, 2000, 1000, 500, 200, 100, 50, 25];
  const counted = manual
    ? Math.round(Number(manualValue) * 100)
    : Object.entries(counts).reduce(
        (n, [denomination, count]) => n + Number(denomination) * count,
        0,
      );
  return (
    <div className={styles.cashCount}>
      {expectedMinor !== undefined && (
        <p>
          {t("ยอดคาดหวัง", "Expected cash")}:{" "}
          <strong>{formatPosMoney(expectedMinor, currency)}</strong>
        </p>
      )}
      <Field orientation="inline" className={styles.check}>
        <TextInput type="checkbox" checked={manual} onChange={(e) => setManual(e.target.checked)} />
        {t("กรอกยอดนับรวม", "Enter counted total")}
      </Field>
      {manual ? (
        <Field>
          {t("เงินสดที่นับได้", "Counted cash")}
          <TextInput
            type="number"
            min="0"
            step="0.01"
            value={manualValue}
            onChange={(e) => setManualValue(e.target.value)}
            required
          />
        </Field>
      ) : (
        <div className={styles.denominations}>
          {denominations.map((value) => (
            <Field key={value}>
              {formatPosMoney(value, currency)}
              <TextInput
                type="number"
                min="0"
                step="1"
                max="100000"
                aria-label={`${t("จำนวน", "Count")} ${value / 100}`}
                value={counts[String(value)] ?? ""}
                onChange={(e) => setCounts((c) => ({ ...c, [value]: Number(e.target.value) }))}
              />
            </Field>
          ))}
        </div>
      )}
      <TextInput type="hidden" name={fieldName} value={(counted / 100).toFixed(2)} />
      {!manual && <TextInput type="hidden" name="denominations" value={JSON.stringify(counts)} />}
      <p>
        {t("รวมเงินสด", "Counted total")}: <strong>{formatPosMoney(counted, currency)}</strong>
        {expectedMinor !== undefined && (
          <>
            {" "}
            · {t("ผลต่าง", "Variance")}: {formatPosMoney(counted - expectedMinor, currency)}
          </>
        )}
      </p>
    </div>
  );
}

export function PosDayClosePanel({
  data,
  api,
  base,
  busy,
  send,
}: {
  data: PosWorkspace;
  api: string;
  base: string;
  busy: boolean;
  send: PosSend;
}) {
  const language = useFrontLanguage(),
    t = (th: string, en: string) => (language === "th" ? th : en);
  const [branch, setBranch] = useState(data.branchId ?? String(data.branches[0]?.id ?? "")),
    [date, setDate] = useState(businessDate()),
    [read, setRead] = useState<{
      branch: string;
      date: string;
      report?: Report;
      error?: string;
    } | null>(null);
  const report = read?.branch === branch && read.date === date ? (read.report ?? null) : null;
  const error = read?.branch === branch && read.date === date ? (read.error ?? "") : "";
  const close = data.dayCloses.find(
    (row) => row.branch_id === branch && row.business_date === date,
  );
  const snapshot = close ? (JSON.parse(String(close.snapshot_json)) as Report) : report;
  const reads = useRef(createLatestRead<Report>());
  useEffect(() => {
    let live = true;
    if (!branch) return;
    const reader = reads.current;
    // Deferring the start also avoids Strict Mode's discarded mount request.
    const timer = setTimeout(() => {
      void reader.read(JSON.stringify([api, branch, date]), async signal => {
        const response = await workFetch(`${api}/day/report?${new URLSearchParams({ branch, date })}`, {signal});
        const body = await response.json();
        if (!response.ok) throw new Error(body.error?.message ?? "Report unavailable");
        return body.data as Report;
      }, report => { if (live) setRead({branch, date, report}); })
        .catch(error => { if (live) setRead({branch, date, error: error instanceof Error ? error.message : "Report unavailable"}); });
    }, 0);
    return () => {
      live = false;
      clearTimeout(timer);
      reader.cancel();
    };
  }, [api, branch, date, data.dayCloses]);
  const money = (value: number) => formatPosMoney(value, snapshot?.currency ?? data.currency);
  const totals = snapshot?.totals;
  const refundTax =
    totals?.refundTaxMinor ??
    snapshot?.sources.refunds
      .filter((row) => row.state === "completed" && row.kind === "sale")
      .reduce((sum, row) => sum + Number(row.tax_minor ?? 0), 0) ??
    0;
  const prepare = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    void send("day.prepare", { branchId: branch, businessDate: date });
  };
  return (
    <section className={styles.panel} data-pos-day-close>
      <h2>{t("ปิดวันและตรวจบัญชี", "Day close & accounting review")}</h2>
      <form className={styles.form} onSubmit={prepare}>
        <Field>
          {t("สาขา", "Branch")}
          <Select value={branch} onChange={(e) => setBranch(e.target.value)}>
            {data.branches.map((row) => (
              <option key={String(row.id)} value={String(row.id)}>
                {String(row.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field>
          {t("วันธุรกิจ", "Business date")}
          <TextInput
            type="date"
            max={businessDate()}
            value={date}
            onChange={(e) => setDate(e.target.value)}
            required
          />
        </Field>
        {!close && data.permissions["pos.approve"] && (
          <Button
            clientManaged type="submit"
            variant="primary"
            className={styles.button}
            disabled={busy || !report || !!report.blockers.length}
          >
            {t("เตรียมรายงานปิดวัน", "Prepare day close")}
          </Button>
        )}
      </form>
      {error && (
        <p role="alert" className={styles.alert}>
          {error}
        </p>
      )}
      {close && (
        <p role="status">
          {t("สถานะ", "Status")}: <strong>{String(close.state)}</strong> ·{" "}
          {t("ผู้เตรียม", "Prepared by")} {String(close.prepared_by)}
          {close.confirmed_by && <> · Finance {String(close.confirmed_by)}</>}
        </p>
      )}
      {close?.state === "prepared" &&
        data.permissions["finance.approve"] &&
        close.prepared_by !== data.userId && (
          <Button
            type="submit"
            variant="primary"
            className={styles.button}
            disabled={busy}
            onClick={() =>
              void send("day.confirm", { id: close.id, expectedVersion: close.version })
            }
          >
            {t("Finance ยืนยันปิดวัน", "Finance · confirm day close")}
          </Button>
        )}
      {close?.state === "prepared" && close.prepared_by === data.userId && (
        <p>{t("รอ Finance อีกคนตรวจและยืนยัน", "Waiting for another Finance reviewer")}</p>
      )}
      {report && report.blockers.length > 0 && (
        <div className={styles.blocker} role="alert">
          <strong>{t("จัดการก่อนปิดวัน", "Resolve before closing")}</strong>
          <ul>
            {report.blockers.map((blocker) => (
              <li key={`${blocker.code}:${blocker.id}`}>
                {blocker.message} · {blocker.id.slice(0, 8)}
              </li>
            ))}
          </ul>
        </div>
      )}
      {!snapshot && !error && branch && (
        <p className={styles.reportLoading} role="status" aria-busy="true">
          {t("กำลังโหลดรายงานของสาขาและวันที่เลือก…", "Loading the selected branch and date…")}
        </p>
      )}
      {totals && (
        <>
          <dl className={styles.dayHighlights}>
            {(
              [
                [t("ยอดสุทธิ", "Net total"), totals.netMinor],
                [t("ต้นทุน", "Cost of goods"), totals.costMinor],
                [t("คืนเงิน", "Refunds"), totals.refundMinor],
                [t("ผลต่างเงินสด", "Cash variance"), totals.cashVarianceMinor],
              ] as Array<[string, number]>
            ).map(([label, value]) => (
              <div key={label}>
                <dt>{label}</dt>
                <dd>{money(value)}</dd>
              </div>
            ))}
          </dl>
          <dl className={styles.daySummary}>
            {(
              [
                [t("ยอดขายก่อนส่วนลด", "Gross sales"), totals.grossMinor],
                [t("ส่วนลด", "Discounts"), totals.discountMinor],
                [t("ยอดขายก่อนภาษี", "Sales excluding tax"), totals.salesMinor],
                [t("ภาษีขาย", "Sales tax"), totals.taxMinor],
                [t("ภาษีคืนสินค้า", "Refund tax"), refundTax],
                [t("ภาษีสุทธิ", "Net tax"), totals.netTaxMinor ?? totals.taxMinor - refundTax],
                [t("ของเสีย", "Waste"), totals.wasteMinor],
                [t("ลงบัญชีห้องพัก · PMS", "Room charges · PMS"), totals.roomChargeMinor],
              ] as Array<[string, number]>
            ).map(([label, value]) => (
              <div key={label}>
                <dt>{label}</dt>
                <dd>{money(value)}</dd>
              </div>
            ))}
          </dl>
          <h3>{t("เงินรับตามช่องทาง", "Receipts by method")}</h3>
          {Object.entries(totals.received).map(([method, value]) => (
            <p key={method}>
              {method}: {money(value)} · {t("คืน", "Refunded")}{" "}
              {money(totals.returned[method] ?? 0)}
            </p>
          ))}
          <p className={styles.muted}>
            {t(
              "การปิดวันเก็บ snapshot และต้นทาง ไม่ลงรายได้ซ้ำ เงิน Stripe รอ payout ตรวจแยกใน Finance",
              "Day close retains a snapshot and sources without posting revenue again. Stripe payout reconciliation is separate in Finance.",
            )}
          </p>
          <details>
            <summary>
              {t(
                "บิล ใบรับเงิน Journal และ Stock movement ต้นทาง",
                "Source bills, receipts, journals & stock movements",
              )}
            </summary>
            <TableFrame label="Records" className={styles.scroll}>
              <Table className={styles.table}>
                <thead>
                  <tr>
                    <th>{t("ต้นทาง", "Source")}</th>
                    <th>{t("เลขที่", "Reference")}</th>
                    <th>{t("เปิดดู", "Open")}</th>
                  </tr>
                </thead>
                <tbody>
                  {snapshot.sources.sales
                    .filter((s) => s.state === "completed")
                    .map((sale) => (
                      <tr key={String(sale.id)}>
                        <td>POS {sale.room_charge_id ? "→ PMS" : ""}</td>
                        <td>{String(sale.number)}</td>
                        <td>
                          <Link href={`${base}/pos?sale=${sale.id}`}>{t("บิล", "Bill")}</Link>
                          {sale.invoice_id && (
                            <>
                              {" "}
                              ·{" "}
                              <Link href={`${base}/finance/invoices/${sale.invoice_id}`}>
                                {t("ใบแจ้งหนี้", "Invoice")}
                              </Link>
                            </>
                          )}
                        </td>
                      </tr>
                    ))}
                  {snapshot.sources.payments
                    .filter((p) => p.finance_payment_id)
                    .map((p) => (
                      <tr key={String(p.id)}>
                        <td>{p.method}</td>
                        <td>{p.sale_number}</td>
                        <td>
                          <Link href={`${base}/finance/payments/${p.finance_payment_id}`}>
                            {t("ใบรับเงิน", "Receipt")}
                          </Link>
                        </td>
                      </tr>
                    ))}
                  {snapshot.sources.journals.map((j) => (
                    <tr key={String(j.id)}>
                      <td>Journal</td>
                      <td>{j.memo}</td>
                      <td>
                        <Link href={`${base}/finance/journals/${j.id}`}>
                          {t("บัญชี", "Ledger")}
                        </Link>
                      </td>
                    </tr>
                  ))}
                  {snapshot.sources.stockMoves.map((m) => (
                    <tr key={String(m.id)}>
                      <td>Stock · {m.kind}</td>
                      <td>
                        {m.source_number} · {Number(m.quantity_milli) / 1000}
                      </td>
                      <td>
                        <Link href={`${base}/pos?movement=${m.id}`}>
                          {t("รายการสต็อก", "Stock source")}
                        </Link>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </TableFrame>
          </details>
          <p>
            {t("ยอดบัญชี Stripe รอรับของทั้งบริษัท", "Company Stripe clearing balance")}:{" "}
            {money(snapshot.stripeClearingMinor ?? 0)}
          </p>
          {snapshot.pendingPayouts.length > 0 && (
            <details>
              <summary>
                {t(
                  "Stripe · รายการรอตรวจ / กระทบยอดของทั้งบริษัท",
                  "Stripe · company entries awaiting review / reconciliation",
                )}{" "}
                ({snapshot.pendingPayouts.length})
              </summary>
              {snapshot.pendingPayouts.map((p) => (
                <p key={String(p.id)}>
                  {p.kind} · {p.state} · {money(Number(p.net_minor))} · {t("ค่าธรรมเนียม", "Fees")}{" "}
                  {money(Number(p.fee_minor))}
                </p>
              ))}
            </details>
          )}
        </>
      )}
    </section>
  );
}
