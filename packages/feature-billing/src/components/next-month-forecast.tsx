"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, TextInput } from "@erpx/ui/components/foundation";

import { betaFetch } from "@erpx/feature-shared/lib/beta-client";
import { calculateSeatForecast, parseForecastBaht, parseForecastGb } from "@erpx/feature-billing/lib/billing-forecast";
import type { SeatForecastBaseline } from "@erpx/server-billing/public/billing-forecast";
import { useEffect, useState } from "react";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import styles from "@erpx/feature-billing/components/next-month-forecast.module.css";

const money = (satang: number) =>
  new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "THB",
  }).format(satang / 100);
const number = (value: number) =>
  new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(value);
const observed = (at: number | null) => (at === null ? "Unavailable" : <TimeText appearance="standard" value={at} />);
const gbInput = (bytes: number | null) =>
  bytes === null ? "" : (Math.round(bytes / 1_000) / 1_000_000).toFixed(6).replace(/\.?0+$/, "");

export function NextMonthForecast({
  base,
  platform,
  holdingId,
}: {
  base: string;
  platform: boolean;
  holdingId: string;
}) {
  const [baseline, setBaseline] = useState<SeatForecastBaseline | null>(null);
  const [seatsInput, setSeatsInput] = useState("");
  const [storageInput, setStorageInput] = useState("");
  const [aiInput, setAiInput] = useState("");
  const [error, setError] = useState("");
  const api = platform
    ? `/api/v1/platform/billing/forecast?holdingId=${encodeURIComponent(holdingId)}`
    : `/api/v1${base}/billing/forecast`;

  useEffect(() => {
    if (!holdingId) return;
    let active = true;
    betaFetch<SeatForecastBaseline>(api)
      .then((result) => {
        if (!active) return;
        setBaseline(result);
        setSeatsInput(result.seats.count === null ? "" : String(result.seats.count));
        setStorageInput(gbInput(result.storage.originalBytes));
        setAiInput("");
        setError("");
      })
      .catch((reason) => {
        if (active)
          setError(reason instanceof Error ? reason.message : "The forecast could not be loaded.");
      });
    return () => {
      active = false;
    };
  }, [api, holdingId]);

  const seats =
    /^\d+$/.test(seatsInput) &&
    Number.isSafeInteger(Number(seatsInput)) &&
    Number(seatsInput) <= 10_000
      ? Number(seatsInput)
      : null;
  const storageBytes = parseForecastGb(storageInput);
  const aiSatang = aiInput.trim() === "" ? null : parseForecastBaht(aiInput);
  let amounts: ReturnType<typeof calculateSeatForecast> | null = null;
  let calculationError = "";
  if (
    baseline?.price &&
    seats !== null &&
    storageBytes !== null &&
    (aiInput.trim() === "" || aiSatang !== null)
  ) {
    try {
      amounts = calculateSeatForecast({
        month: baseline.forecastMonth,
        price: baseline.price,
        seats,
        averageOriginalBytes: storageBytes,
        assumedProviderAiCostSatang: aiSatang,
      });
    } catch {
      calculationError = "These inputs are outside the supported forecast range.";
    }
  }
  const seatMax = Math.min(10_000, Math.max(500, (baseline?.seats.count ?? 0) * 2, seats ?? 0));
  const includedGb =
    baseline?.price && seats !== null
      ? (seats * baseline.price.includedOriginalBytes) / 1_000_000_000
      : 0;
  const storageMax = Math.min(
    100_000,
    Math.max(
      10,
      includedGb * 1.5,
      storageBytes === null ? 0 : Number(storageBytes) / 1_000_000_000,
    ),
  );
  const stale =
    baseline?.seats.observedAt !== null &&
    baseline?.seats.observedAt !== undefined &&
    baseline.generatedAt - baseline.seats.observedAt > 86_400_000;

  return (
    <section className={styles.root} aria-label="Next-month seat forecast">
      <header className={styles.header}>
        <div>
          <h3>Next-month seat forecast</h3>
          <p>
            What-if forecast — not an invoice. Assume the selected seats and original files stay
            constant for the full Bangkok calendar month.
          </p>
        </div>
        <span className={styles.badge}>SANDBOX WHAT-IF</span>
      </header>
      {error && (
        <InlineNotice tone="danger" role="alert" className={styles.warning}>
          {error}
        </InlineNotice>
      )}
      {!error && !baseline && <InlineNotice tone="info" role="status">Loading current seat and file measurements…</InlineNotice>}
      {baseline && (
        <>
          <p className={styles.context}>
            Forecast month <strong>{baseline.forecastMonth}</strong> · Price version{" "}
            <strong>{baseline.price?.id ?? "unavailable"}</strong>
          </p>
          {!baseline.price && (
            <InlineNotice tone="danger" role="alert" className={styles.warning}>
              A seat-only Sandbox price is unavailable. No amount can be calculated.
            </InlineNotice>
          )}
          {baseline.price && !baseline.priceEffectiveForMonth && (
            <p className={styles.warning}>
              This price starts in {baseline.price.effectiveMonth}; it does not apply to the
              forecast month. The comparison is hypothetical and grants no storage entitlement.
            </p>
          )}
          {stale && (
            <p className={styles.warning}>
              The seat snapshot is older than 24 hours. Check the as-of time before using its count.
            </p>
          )}
          {baseline.storage.coverage !== "measured" && (
            <p className={styles.warning}>
              Original-file measurement is {baseline.storage.coverage}; the starting storage amount
              may omit files. Adjust it to explore a scenario.
            </p>
          )}
          <div className={styles.controls}>
            <div className={styles.control}>
              <label htmlFor={`${platform ? "platform" : "holding"}-forecast-seats`}>
                Active named seats for the full month
              </label>
              <div className={styles.inputRow}>
                <TextInput appearance="standard"
                  id={`${platform ? "platform" : "holding"}-forecast-seats`}
                  type="number"
                  min="0"
                  max="10000"
                  step="1"
                  value={seatsInput}
                  onChange={(event) => setSeatsInput(event.target.value)}
                  placeholder="Enter seats"
                />
                <span>seats</span>
              </div>
              <TextInput appearance="standard"
                aria-label="Adjust forecast seats"
                type="range"
                min="0"
                max={seatMax}
                step="1"
                value={seats ?? 0}
                onChange={(event) => setSeatsInput(event.target.value)}
              />
              <small>
                Measured now:{" "}
                {baseline.seats.count === null ? "Unavailable" : number(baseline.seats.count)} · As
                of {observed(baseline.seats.observedAt)}
                {stale ? " · Older than 24 hours" : ""}
              </small>
              {seatsInput && seats === null && (
                <small className={styles.warning}>Enter a whole number from 0 to 10,000.</small>
              )}
            </div>
            <div className={styles.control}>
              <label htmlFor={`${platform ? "platform" : "holding"}-forecast-storage`}>
                Average original files next month
              </label>
              <div className={styles.inputRow}>
                <TextInput appearance="standard"
                  id={`${platform ? "platform" : "holding"}-forecast-storage`}
                  type="number"
                  min="0"
                  max="999999"
                  step="0.000001"
                  value={storageInput}
                  onChange={(event) => setStorageInput(event.target.value)}
                  placeholder="Enter storage"
                />
                <span>decimal GB</span>
              </div>
              <TextInput appearance="standard"
                aria-label="Adjust forecast original storage"
                type="range"
                min="0"
                max={storageMax}
                step="any"
                value={storageBytes === null ? 0 : Number(storageBytes) / 1_000_000_000}
                onChange={(event) =>
                  setStorageInput(
                    Number(event.target.value)
                      .toFixed(6)
                      .replace(/\.?0+$/, ""),
                  )
                }
              />
              <small>
                Current original files:{" "}
                {baseline.storage.originalBytes === null
                  ? "Unavailable"
                  : `${gbInput(baseline.storage.originalBytes)} GB`}{" "}
                · {baseline.storage.coverage} coverage · As of{" "}
                {observed(baseline.storage.observedAt)}
              </small>
              {storageInput && storageBytes === null && (
                <small className={styles.warning}>
                  Enter a nonnegative decimal GB amount with up to six places.
                </small>
              )}
            </div>
            <div className={styles.control}>
              <label htmlFor={`${platform ? "platform" : "holding"}-forecast-ai`}>
                Assumed ERPX-funded AI provider cost (optional)
              </label>
              <div className={styles.inputRow}>
                <TextInput appearance="standard"
                  id={`${platform ? "platform" : "holding"}-forecast-ai`}
                  type="number"
                  min="0"
                  max="999999999"
                  step="0.01"
                  value={aiInput}
                  onChange={(event) => setAiInput(event.target.value)}
                  placeholder="Leave blank to exclude AI"
                />
                <span>THB</span>
              </div>
              <small>
                ERPX adds {baseline.price?.aiMarkupPercent ?? "—"}% to this assumption. BYOK is
                never charged by ERPX.
              </small>
              {aiInput.trim() !== "" && aiSatang === null && (
                <small className={styles.warning}>Enter THB with at most two decimal places.</small>
              )}
            </div>
          </div>
          <Button appearance="standard"
            variant="ghost"
            className={styles.reset}
            type="button"
            onClick={() => {
              setSeatsInput(baseline.seats.count === null ? "" : String(baseline.seats.count));
              setStorageInput(gbInput(baseline.storage.originalBytes));
              setAiInput("");
            }}
          >
            Reset to measured values
          </Button>
          {calculationError && (
            <InlineNotice tone="danger" role="alert" className={styles.warning}>
              {calculationError}
            </InlineNotice>
          )}
          {amounts && (
            <div className={styles.result} aria-live="polite">
              <div>
                <span>Seats ({number(seats!)})</span>
                <strong>{money(amounts.seatSatang)}</strong>
              </div>
              <div>
                <span>Original-file allowance used in this calculation</span>
                <strong>{number(amounts.includedOriginalGb)} GB-months</strong>
              </div>
              <div>
                <span>Excess storage</span>
                <strong>{money(amounts.storageSatang)}</strong>
              </div>
              <div className={styles.subtotal}>
                <span>Seat + storage subtotal</span>
                <strong>{money(amounts.subtotalSatang)}</strong>
              </div>
              <div>
                <span>ERPX-funded AI charge</span>
                <strong>
                  {amounts.aiSatang === null ? "Not estimated" : money(amounts.aiSatang)}
                </strong>
              </div>
              {amounts.totalSatang !== null && (
                <div className={styles.total}>
                  <span>What-if total — not an invoice</span>
                  <strong>{money(amounts.totalSatang)}</strong>
                </div>
              )}
              {amounts.totalSatang === null && (
                <p>Enter an AI provider-cost assumption, including 0, to see a scenario total.</p>
              )}
            </div>
          )}
          {!amounts && baseline.price && (
            <p className={styles.warning}>
              Enter both seats and storage to calculate a subtotal. Missing measurements are never
              treated as zero.
            </p>
          )}
          <p className={styles.footnote}>
            What-if forecast — not an invoice. This excludes the current Support plan, setup,
            additional services, tax, payment fees, and customer-paid BYOK. The proposed storage
            allowance is not enabled.
          </p>
        </>
      )}
    </section>
  );
}
