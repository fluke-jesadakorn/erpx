"use client";
import { InlineNotice } from "@/components/ui/foundation";


import { useState } from "react";
import { Button, Field, TextArea } from "@/components/ui/foundation";
import { usePeopleCommand } from "./payroll-controls";
import { Card, FormLayout } from "@/components/ui/foundation";

export function PayrollCancellationControls({ base, runId, version }: { base: string; runId: string; version: number }) {
  const [open, setOpen] = useState(false);
  const { send, busy, error } = usePeopleCommand(base);
  return <Card as="section" appearance="standard" className="ui-panel" aria-label="Cancel payroll draft">
    <Button appearance="standard" type="button" variant="secondary" onClick={() => setOpen(value => !value)} disabled={busy}>
      {open ? "Keep payroll draft" : "Cancel payroll draft"}
    </Button>
    {open && <FormLayout className="ui-form-grid" onSubmit={async event => {
      event.preventDefault();
      const form = new FormData(event.currentTarget);
      if (await send(`payroll/runs/${runId}/cancel`, { expectedVersion: version, reason: form.get("reason") })) setOpen(false);
    }}>
      <p>The cancelled run retains its original calculation and review copies. A new run can use this work period.</p>
      <Field appearance="standard">{"Cancellation reason"}<TextArea appearance="standard" name="reason" minLength={8} maxLength={1000} required /></Field>
      <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>Confirm payroll cancellation</Button>
    </FormLayout>}
    {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
  </Card>;
}
