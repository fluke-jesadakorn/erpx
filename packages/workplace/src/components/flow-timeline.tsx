"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { Button, Field, Select, TextInput } from "@erpx/ui/components/foundation";

import { useEffect, useRef, useState } from "react";
import {createFlowAssignmentCommands} from "../lib/flow-assignment-command";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import type { FlowView } from "@erpx/feature-shared/lib/flow";
import styles from "@erpx/workplace/components/flow.module.css";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";

export function FlowTimeline({ flow, endpoint }: { flow: FlowView; endpoint: string }) {
  const router = useRouter();
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const commands = useRef<ReturnType<typeof createFlowAssignmentCommands> | null>(null);
  commands.current ??= createFlowAssignmentCommands();
  const [uncertain, setUncertain] = useState(false);
  useEffect(() => { try { setUncertain(!!commands.current?.pending(endpoint)); } catch { setUncertain(false); } }, [endpoint]);
  async function assign(event: React.FormEvent<HTMLFormElement>, stepKey: string) {
    event.preventDefault();
    const element=event.currentTarget,form = new FormData(element);
    setBusy(true); setError("");
    try {
      const result=await commands.current!.submit(endpoint,flow.kind,flow.recordId,flow.version,{
        stepKey,userId:form.get("userId"),reason:form.get("reason"),saveDefault:form.get("saveDefault") === "on",
        expectedVersion:flow.version,recordVersion:flow.recordVersion,
      },{form:element,formEditVersion:element.getAttribute('data-erpx-edit-version')});
      setUncertain(false); if(result.unchanged)router.refresh();
    } catch (cause) {
      setError((cause as Error).message);
      try { setUncertain(!!commands.current!.pending(endpoint)); } catch { setUncertain(true); }
    } finally { setBusy(false); }
  }
  async function retry() {
    setBusy(true);setError("");
    try { const result=await commands.current!.retry(endpoint);setUncertain(false);if(result.unchanged)router.refresh(); }
    catch(cause){setError((cause as Error).message);try{setUncertain(!!commands.current!.pending(endpoint));}catch{setUncertain(true);}}
    finally{setBusy(false);}
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
      {uncertain && <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={() => void retry()}>Retry original owner assignment</Button>}
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
                <FormDisclosure appearance="standard" retainFormContents={uncertain}>
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
                      disabled={busy || uncertain || !step.candidates.length}
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
