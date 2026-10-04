"use client";
import { Button, Field, Select } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work-fetch";
import { useState } from "react";
import { type UiAppearance } from "@/components/ui/foundation";
export function SpendingReadiness({ appearance = "legacy", base }: ({ base: string }) & { appearance?: UiAppearance }) {
  const [kind, setKind] = useState("expense"),
    [result, setResult] = useState(""),
    [busy, setBusy] = useState(false);
  async function check() {
    setBusy(true);
    try {
      const response = await workFetch(`/api/v1${base}/people/organization/readiness?kind=${kind}`);
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Readiness check failed.");
      setResult(
        body.data.ready
          ? "Your staff profile and independent approval team are ready. Each request still needs its supporting details and documents."
          : body.data.blockedReason,
      );
    } catch (e) {
      setResult(e instanceof Error ? e.message : "Readiness check failed.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <h2 data-ui-appearance={appearance}>Check my spending readiness</h2>
      <p data-ui-appearance={appearance} className="muted">
        Checks your reporting route and independent Accounting and Finance participants.
      </p>
      <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-grid" : "business-form"}>
        <Field appearance={appearance}>
          Request type
          <Select appearance={appearance}
            value={kind}
            onChange={(e) => {
              setKind(e.target.value);
              setResult("");
            }}
          >
            <option data-ui-appearance={appearance} value="expense">Expense claim</option>
            <option data-ui-appearance={appearance} value="purchase">Purchase request</option>
            <option data-ui-appearance={appearance} value="payment">Payment request</option>
          </Select>
        </Field>
        <Button appearance={appearance} variant="secondary" type="button" disabled={busy} onClick={() => void check()}>
          {busy ? "Checking…" : "Check readiness"}
        </Button>
      </div>
      {result && <p data-ui-appearance={appearance} role="status">{result}</p>}
    </section>
  );
}
