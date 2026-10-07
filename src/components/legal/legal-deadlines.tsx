"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import { previewDeadline } from "@/server/modules/legal/public/rules/deadlines";
import s from "./legal.module.css";
export function LegalDeadlines() {
  const [result, setResult] = useState<ReturnType<typeof previewDeadline> | null>(null),
    [error, setError] = useState("");
  return (
    <FormDisclosure appearance="standard" className={s.event}>
      <summary>Review a deadline calculation</summary>
      <p>
        Use an explicit reviewed rule. This calculator does not determine a legal limitation period.
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
        <Field appearance="standard" className={s.field}>
          Start local date
          <TextInput appearance="standard" name="start" type="date" required />
        </Field>
        <Field appearance="standard" className={s.field}>
          Period
          <TextInput appearance="standard" name="amount" type="number" min={0} max={3660} required />
        </Field>
        <Field appearance="standard" className={s.field}>
          Counting method
          <Select appearance="standard" name="unit">
            <option value="calendar_days">Calendar days</option>
            <option value="business_days">Monday–Friday excluding supplied holidays</option>
            <option value="months">Months, clamp to month end</option>
          </Select>
        </Field>
        <Field appearance="standard" className={s.field}>
          Reviewed holidays, YYYY-MM-DD separated by spaces
          <TextArea appearance="standard" name="holidays" />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button}>
          Preview date
        </Button>
      </form>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {result && (
        <p className={s.notice}>
          Proposed date: <strong>{result.date}</strong>. {result.convention} Save this date to the
          relevant record or workflow only after review.
        </p>
      )}
    </FormDisclosure>
  );
}
