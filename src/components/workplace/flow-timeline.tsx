"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work_projection/work-fetch";
import { useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import type { FlowView } from "@/lib/shared/flow";
import styles from "./flow.module.css";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";

export function FlowTimeline({ flow, endpoint }: { flow: FlowView; endpoint: string }) {
  const router = useRouter();
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function assign(event: React.FormEvent<HTMLFormElement>, stepKey: string) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({
          stepKey,
          userId: form.get("userId"),
          reason: form.get("reason"),
          saveDefault: form.get("saveDefault") === "on",
          expectedVersion: flow.version,
          recordVersion: flow.recordVersion,
        }),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Assignment failed.");
      router.refresh();
    } catch (cause) {
      setError((cause as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section
      id="workflow-owners"
      tabIndex={-1}
      className={`card ${styles.flow}`}
      aria-label="Workflow and next responsible staff"
    >
      <header>
        <div>
          <p className="eyebrow">Flow · Teams & owners</p>
          <h2>{flow.current?.action ?? "No pending action"}</h2>
        </div>
        <span className="ui-badge">Flow status: {flow.state.replaceAll("_", " ")}</span>
      </header>
      {flow.current && (
        <p>
          Now: <strong>{flow.current.primary?.name ?? "No owner assigned"}</strong> ·{" "}
          {flow.current.teamName} · {flow.current.role.replaceAll("_", " ")}
          {flow.canAct && <strong> · Your task</strong>}
        </p>
      )}
      {flow.next && (
        <p className="muted">
          Next: {flow.next.action} → {flow.next.teamName} /{" "}
          {flow.next.primary?.name ?? "Waiting for owner"}
        </p>
      )}
      {flow.blocker && (
        <InlineNotice tone="info" role="status" className="notice">
          {flow.blocker}
        </InlineNotice>
      )}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <ol className={styles.steps}>
        {flow.steps.map((step, index) => (
          <li
            key={step.key}
            className={step.status === "current" ? styles.current : undefined}
            aria-current={step.status === "current" ? "step" : undefined}
          >
            <span className={styles.number}>{index + 1}</span>
            <div>
              <strong>{step.action}</strong>
              <p>
                {step.teamName} · {step.primary?.name ?? "Unassigned"}
              </p>
              <small>
                {step.role.replaceAll("_", " ")} ·{" "}
                {step.status === "current"
                  ? "Pending"
                  : step.status === "recorded"
                    ? "Recorded"
                    : "Flow step"}
              </small>
              {flow.current && step.blocker && step.status !== "recorded" && (
                <p className="muted">{step.blocker}</p>
              )}
              {flow.canManage && flow.current && index >= flow.steps.indexOf(flow.current) && (
                <FormDisclosure appearance="standard">
                  <summary>Assign / change owner</summary>
                  <form onSubmit={(event) => void assign(event, step.key)}>
                    <Field appearance="standard">
                      Owner
                      <Select appearance="standard"
                        name="userId"
                        required
                        defaultValue={step.primary?.userId ?? ""}
                        disabled={busy}
                      >
                        <option value="">Select eligible person</option>
                        {step.candidates.map((p) => (
                          <option key={p.userId} value={p.userId}>
                            {p.name} · {p.teamName}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Reason
                      <TextInput appearance="standard"
                        name="reason"
                        required
                        minLength={3}
                        maxLength={2000}
                        disabled={busy}
                      />
                    </Field>
                    <Field appearance="standard">Apply this owner change<Select appearance="standard" name="saveDefault" defaultValue="record" disabled={busy}><option value="record">This record only</option><option value="on">This record and future documents in this scope</option></Select><small>Future defaults never rewrite other submitted routes.</small></Field>
                    <Button appearance="standard"
                      clientManaged type="submit"
                      variant="secondary"
                      disabled={busy || !step.candidates.length}
                    >
                      Save owner
                    </Button>
                  </form>
                </FormDisclosure>
              )}
            </div>
          </li>
        ))}
      </ol>
      {!!flow.history.length && (
        <details>
          <summary>Handoff history ({flow.history.length})</summary>
          <ul>
            {flow.history.map((e, index) => (
              <li key={index}>
                {e.actorName} · {e.action} · <TimeText appearance="standard" value={e.at} />
                {e.reason && ` · ${e.reason}`}
              </li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}
