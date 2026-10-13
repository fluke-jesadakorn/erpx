"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { payrollErrorMessage, type PayrollApiError } from "@/lib/people_payroll/payroll-errors";
import { Card, FormLayout } from "@/components/ui/foundation";
type Schedule = {
  id: string;
  frequency: string;
  timezone: string;
  currency: string;
  enabled: boolean;
  payDelayDays: number;
  anchorDate: string | null;
  version: number;
};
export function PayrollScheduleSettings({
  base,
  schedules,
  names,
}: {
  base: string;
  userId: string;
  schedules: Schedule[];
  names: Record<string, string>;
}) {
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>{"Scheduled pay runs"}</h2>
      <p className="muted">
        {
          "Drafts and previews are prepared automatically. Approval, posting, and payment remain manual."
        }
      </p>
      {schedules.length ? (
        schedules.map((schedule) => (
          <ScheduleForm
            key={schedule.id}
            base={base}
            schedule={schedule}
            name={names[schedule.id] ?? schedule.id}
          />
        ))
      ) : (
        <p>{"Create a pay group first."}</p>
      )}
    </Card>
  );
}
function ScheduleForm({
  base,
  schedule,
  name,
}: {
  base: string;
  schedule: Schedule;
  name: string;
}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [enabled, setEnabled] = useState(schedule.enabled);
  const attempt = useRef<{
    body: string;
    key: string;
  } | null>(null);
  const frequency = schedule.frequency;
  async function save(form: HTMLFormElement) {
    const data = new FormData(form);
    const input = {
      enabled,
      payDelayDays: Number(data.get("payDelayDays")),
      anchorDate: String(data.get("anchorDate") ?? "") || null,
      expectedVersion: schedule.version,
    };
    const body = JSON.stringify(input),
      path = `pay-groups/${encodeURIComponent(schedule.id)}/schedule`;
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/people/${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body,
      });
      const result = (await response.json()) as {
        error?: PayrollApiError;
      };
      if (!response.ok) throw result.error ?? { message: "Could not save the schedule." };
      attempt.current = null;
      router.refresh();
    } catch (cause) {
      setError(payrollErrorMessage(cause, "Could not save the schedule."));
    } finally {
      setBusy(false);
    }
  }
  return (
    <FormLayout
      className="ui-form-grid"
      onSubmit={(event) => {
        event.preventDefault();
        void save(event.currentTarget);
      }}
    >
      <h3>
        {name} · {schedule.currency} · {frequency}
      </h3>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <Field appearance="standard" orientation="inline">
        <TextInput appearance="standard"
          type="checkbox"
          checked={enabled}
          onChange={(event) => setEnabled(event.currentTarget.checked)}
        />{" "}
        {"Create scheduled drafts"}
      </Field>
      <Field appearance="standard">
        {"Days after period end"}
        <TextInput appearance="standard"
          name="payDelayDays"
          type="number"
          min="1"
          max="45"
          step="1"
          required
          defaultValue={schedule.payDelayDays}
        />
      </Field>
      {["weekly", "fortnightly"].includes(schedule.frequency) && (
        <Field appearance="standard">
          {"Schedule anchor date"}
          <TextInput appearance="standard"
            name="anchorDate"
            type="date"
            required={enabled}
            defaultValue={schedule.anchorDate ?? ""}
          />
        </Field>
      )}
      <p className="muted">
        {"Timezone"}: {schedule.timezone} · {"Dates use Gregorian years."}
      </p>
      <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
        {busy ? "Saving…" : "Save schedule"}
      </Button>
    </FormLayout>
  );
}
