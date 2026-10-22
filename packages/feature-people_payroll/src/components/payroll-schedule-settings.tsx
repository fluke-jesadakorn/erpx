"use client";
import {PayrollCommandRecovery} from "@erpx/feature-people_payroll/components/payroll-command-recovery";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextInput } from "@erpx/ui/components/foundation";

import { useState } from "react";
import {usePayrollCommand} from "@erpx/feature-people_payroll/components/use-payroll-command";
import {PayrollForm} from "@erpx/feature-people_payroll/components/payroll-draft-boundary";
import { Card } from "@erpx/ui/components/foundation";
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
  const {send, busy, error, notice, canPrepare, canSettle, recovery} = usePayrollCommand(base, `schedule:${schedule.id}`, JSON.stringify([schedule]));
  const [enabled, setEnabled] = useState(schedule.enabled);
  const frequency = schedule.frequency;
  async function save(form: HTMLFormElement) {
    const data = new FormData(form);
    await send(`pay-groups/${encodeURIComponent(schedule.id)}/schedule`, {
      enabled, payDelayDays: Number(data.get("payDelayDays")), anchorDate: String(data.get("anchorDate") ?? "") || null, expectedVersion: schedule.version,
    });
  }
  return (
    <PayrollForm
      className="ui-form-grid"
      onSubmit={(event) => {
        event.preventDefault();
        void save(event.currentTarget);
      }}
    >
      <h3>
        {name} · {schedule.currency} · {frequency}
      </h3>
      <PayrollCommandRecovery {...recovery} appearance="standard"/>{error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{notice && <p role="status">{notice}</p>}
      <Field appearance="standard" orientation="inline">
        <TextInput appearance="standard"
          type="checkbox"
          checked={enabled}
          disabled={!canPrepare && !schedule.enabled || !canSettle}
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
      <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy || (enabled ? !canPrepare : !canSettle)}>
        {busy ? "Saving…" : "Save schedule"}
      </Button>
    </PayrollForm>
  );
}
