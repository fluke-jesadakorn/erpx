"use client";
import {PayrollCommandRecovery} from "./payroll-command-recovery";
import { InlineNotice } from "@/components/ui/foundation";


import { useState } from "react";
import { Button, Field, TextArea } from "@/components/ui/foundation";
import {usePayrollCommand as usePeopleCommand} from "./use-payroll-command";
import { Card } from "@/components/ui/foundation";

import {PayrollForm, PayrollFormDrawer} from "./payroll-draft-boundary";

export function PayrollCancellationControls({ base, runId, version }: { base: string; runId: string; version: number }) {
  const [open, setOpen] = useState(false);
  const { send, busy, error, notice, canSettle, recovery} = usePeopleCommand(base, `run:${runId}:cancel`, JSON.stringify([runId, version]));
  if (!canSettle) return null;
  return <Card as="section" appearance="standard" className="ui-panel" aria-label="Cancel payroll draft">
    <Button appearance="standard" type="button" variant="secondary" onClick={() => setOpen(value => !value)} disabled={busy}>
      {open ? "Keep payroll draft" : "Cancel payroll draft"}
    </Button>
    {open && <PayrollFormDrawer open title="Cancel payroll draft" onClose={() => setOpen(false)} dismissDisabled={busy}><PayrollForm className="ui-form-grid" onSubmit={async event => {
      event.preventDefault();
      const form = new FormData(event.currentTarget);
      if (await send(`payroll/runs/${runId}/cancel`, { expectedVersion: version, reason: form.get("reason") })) setOpen(false);
    }}>
      <p>The cancelled run retains its original calculation and review copies. A new run can use this work period.</p>
      <Field appearance="standard">{"Cancellation reason"}<TextArea appearance="standard" name="reason" minLength={8} maxLength={1000} required /></Field>
      <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>Confirm payroll cancellation</Button>
    </PayrollForm></PayrollFormDrawer>}
    <PayrollCommandRecovery {...recovery} appearance="standard"/>{error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{notice && <p role="status">{notice}</p>}
  </Card>;
}
