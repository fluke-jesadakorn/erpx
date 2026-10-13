"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work_projection/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Card, FormLayout } from "@/components/ui/foundation";

type Row = Record<string, string | number | null>;

export function PaymentCorrection({
  base,
  payment,
  periods,
}: {
  base: string;
  payment: Row;
  periods: Row[];
}) {
  const router = useRouter(),
    attempt = useRef<{ body: string; key: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function submit(form: HTMLFormElement) {
    const values = new FormData(form),
      body = JSON.stringify({
        expectedVersion: Number(payment.version),
        periodId: values.get("periodId"),
        entryDate: values.get("entryDate"),
        reason: values.get("reason"),
      });
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(
          `${base.replace("/holdings/", "/api/v1/holdings/")}/finance/payments/${payment.id}/reverse`,
          {
            method: "POST",
            headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
            body,
          },
        ),
        result = (await response.json()) as { error?: { message: string } };
      if (!response.ok) throw new Error(result.error?.message ?? "Unable to reverse the payment.");
      attempt.current = null;
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to reverse the payment.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Correct posted payment</h2>
      <p className="muted">
        Creates a posted reversing journal, restores every invoice allocation, and preserves the
        original payment as voided evidence. Reconciled bank items must be unmatched first.
      </p>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      <FormLayout
        className="ui-form-grid"
        onSubmit={(event) => {
          event.preventDefault();
          void submit(event.currentTarget);
        }}
      >
        <Field appearance="standard">
          Correction period
          <Select appearance="standard" name="periodId" required defaultValue="">
            <option value="">Select open period</option>
            {periods.map((period) => (
              <option value={String(period.id)} key={String(period.id)}>
                {String(period.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Correction date
          <TextInput appearance="standard"
            name="entryDate"
            type="date"
            required
            defaultValue={new Date().toISOString().slice(0, 10)}
          />
        </Field>
        <Field appearance="standard">
          Correction evidence
          <TextInput appearance="standard" name="reason" required minLength={3} maxLength={500} />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
          {busy ? "Reversing…" : "Reverse payment and allocations"}
        </Button>
      </FormLayout>
    </Card>
  );
}
