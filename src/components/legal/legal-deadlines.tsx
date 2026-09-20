"use client";
import { useState } from "react";
import { previewDeadline } from "@/lib/legal/deadlines";
import s from "./legal.module.css";
export function LegalDeadlines() {
  const [result, setResult] = useState<ReturnType<
      typeof previewDeadline
    > | null>(null),
    [error, setError] = useState("");
  return (
    <details className={s.event}>
      <summary>Review a deadline calculation</summary>
      <p>
        Use an explicit reviewed rule. This calculator does not determine a
        legal limitation period.
      </p>
      <form
        className={s.detail}
        onSubmit={(e) => {
          e.preventDefault();
          try {
            const v = Object.fromEntries(new FormData(e.currentTarget));
            setResult(
              previewDeadline({
                start: v.start,
                amount: v.amount,
                unit: v.unit,
                holidays: String(v.holidays)
                  .split(/[\s,]+/)
                  .filter(Boolean),
              }),
            );
            setError("");
          } catch (e) {
            setError((e as Error).message);
          }
        }}
      >
        <label className={s.field}>
          Start local date
          <input name="start" type="date" required />
        </label>
        <label className={s.field}>
          Period
          <input name="amount" type="number" min={0} max={3660} required />
        </label>
        <label className={s.field}>
          Counting method
          <select name="unit">
            <option value="calendar_days">Calendar days</option>
            <option value="business_days">
              Monday–Friday excluding supplied holidays
            </option>
            <option value="months">Months, clamp to month end</option>
          </select>
        </label>
        <label className={s.field}>
          Reviewed holidays, YYYY-MM-DD separated by spaces
          <textarea name="holidays" />
        </label>
        <button className={s.button}>Preview date</button>
      </form>
      {error && <p role="alert">{error}</p>}
      {result && (
        <p className={s.notice}>
          Proposed date: <strong>{result.date}</strong>. {result.convention}{" "}
          Save this date to the relevant record or workflow only after review.
        </p>
      )}
    </details>
  );
}
