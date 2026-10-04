"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useCallback, useEffect, useState } from "react";
import type { releaseAcceptance } from "@/server/release-acceptance";
import type { legalRuntimeHealth } from "@/server/legal/runtime-gate";
import { legalProductionScope } from "@/data/production-scope";
import s from "./legal.module.css";

type Status = Awaited<ReturnType<typeof releaseAcceptance>>;
type Runtime = Awaited<ReturnType<typeof legalRuntimeHealth>>;
export function LegalReleaseAcceptance({
  call,
  userId,
}: {
  call: (path: string, body?: unknown) => Promise<unknown>;
  userId: string;
}) {
  const [status, setStatus] = useState<Status | null>(null);
  const [runtime, setRuntime] = useState<Runtime | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(async () => {
    const [acceptance, health] = await Promise.all([
      call("/release-acceptance"),
      call("/runtime-health"),
    ]);
    setStatus(acceptance as Status);
    setRuntime(health as Runtime);
  }, [call]);
  useEffect(() => {
    let active = true;
    queueMicrotask(() => {
      if (active)
        void refresh().catch((error) => {
          if (active) setError(error.message);
        });
    });
    return () => {
      active = false;
    };
  }, [refresh]);
  async function submit(path: string, body: unknown) {
    setBusy(true);
    setError("");
    try {
      await call(path, body);
      await refresh();
    } catch (error) {
      setError(error instanceof Error ? error.message : "Unable to record acceptance.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className={s.detail}>
      <p>
        Legal opens separately for each Company after independent acceptance of the current build,
        configuration and evidence. Legal content and recovery sign-off require named reviewers.
      </p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {!status && !error && <InlineNotice tone="info" role="status">Loading Company readiness…</InlineNotice>}
      {status && (
        <>
          <p>
            <strong>
              {status.state === "passed" ? "Company release accepted" : "Company release pending"}
            </strong>{" "}
            · {legalProductionScope.version}
          </p>
          <dl className={s.evidenceFields}>
            <div>
              <dt>Build</dt>
              <dd className={s.body}>{status.inputs.softwareVersion}</dd>
            </div>
            <div>
              <dt>Configuration revision</dt>
              <dd>{status.inputs.configurationRevision}</dd>
            </div>
            <div>
              <dt>Company setup</dt>
              <dd>
                {status.inputs.setupAccepted ? "Independently accepted" : "Acceptance pending"}
              </dd>
            </div>
          </dl>
          {status.blockers.length > 0 && (
            <ul>
              {status.blockers.map((blocker) => (
                <li key={blocker}>{blocker}</li>
              ))}
            </ul>
          )}
          {status.gates.map((gate) => (
            <article className={s.event} key={gate.id}>
              <h3>{gate.label}</h3>
              <p>
                {gate.owner} · {gate.state}
              </p>
              {gate.reason && <p>{gate.reason}</p>}
              {status.release &&
                status.release.requestedBy !== userId &&
                !status.release.revokedAt && (
                  <form
                    className={s.detail}
                    onSubmit={(event) => {
                      event.preventDefault();
                      const values = new FormData(event.currentTarget);
                      void submit(`/release-acceptance/${status.release!.id}/review`, {
                        expectedVersion: status.release!.version,
                        criterionId: gate.id,
                        state: values.get("state"),
                        reason: values.get("reason"),
                      });
                    }}
                  >
                    <Field appearance="standard" className={s.field}>
                      Acceptance decision
                      <Select appearance="standard" name="state">
                        <option value="pending">Pending or needs corrections</option>
                        <option value="passed">Accept reviewed evidence</option>
                      </Select>
                    </Field>
                    <Field appearance="standard" className={s.field}>
                      Independent review reason
                      <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
                    </Field>
                    <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
                      Record acceptance decision
                    </Button>
                  </form>
                )}
            </article>
          ))}
          <FormDisclosure appearance="standard" className={s.event}>
            <summary>Submit evidence for a new Company release</summary>
            <form
              className={s.detail}
              onSubmit={(event) => {
                event.preventDefault();
                const values = new FormData(event.currentTarget);
                void submit("/release-acceptance", {
                  scopeVersion: legalProductionScope.version,
                  softwareVersion: status.inputs.softwareVersion,
                  expectedConfigurationRevision: status.inputs.configurationRevision,
                  expectedSetupVersion: status.inputs.setupVersion,
                  reason: values.get("reason"),
                  evidence: status.gates.map((gate) => ({
                    criterionId: gate.id,
                    summary: values.get(`${gate.id}:summary`),
                    files: [
                      {
                        reference: values.get(`${gate.id}:reference`),
                        sha256: values.get(`${gate.id}:sha256`),
                      },
                    ],
                  })),
                });
              }}
            >
              <p className={s.small}>
                A Company administrator submits retained evidence; a different person with each
                review permission accepts it. Owner rehearsal does not certify legal content.
              </p>
              {status.gates.map((gate) => (
                <fieldset key={gate.id} className={s.detail}>
                  <legend>{gate.owner}</legend>
                  <p>{gate.label}</p>
                  <Field appearance="standard" className={s.field}>
                    Evidence summary
                    <TextArea appearance="standard" name={`${gate.id}:summary`} minLength={3} maxLength={2000} required />
                  </Field>
                  <Field appearance="standard" className={s.field}>
                    Retained evidence reference
                    <TextInput appearance="standard" name={`${gate.id}:reference`} required maxLength={1000} />
                  </Field>
                  <Field appearance="standard" className={s.field}>
                    Evidence SHA-256
                    <TextInput appearance="standard" name={`${gate.id}:sha256`} required pattern="[a-f0-9]{64}" />
                  </Field>
                </fieldset>
              ))}
              <Field appearance="standard" className={s.field}>
                Release submission reason
                <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
              </Field>
              <Button appearance="standard"
                clientManaged type="submit"
                variant="ghost"
                className={s.button}
                disabled={
                  busy ||
                  status.inputs.softwareVersion === "unbuilt" ||
                  !status.inputs.setupAccepted
                }
              >
                Submit Company release for independent review
              </Button>
            </form>
          </FormDisclosure>
          {status.release && !status.release.revokedAt && (
            <form
              className={s.detail}
              onSubmit={(event) => {
                event.preventDefault();
                const values = new FormData(event.currentTarget);
                void submit(`/release-acceptance/${status.release!.id}/revoke`, {
                  expectedVersion: status.release!.version,
                  reason: values.get("reason"),
                });
              }}
            >
              <Field appearance="standard" className={s.field}>
                Reason to close the release gate
                <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
                Revoke Company release acceptance
              </Button>
            </form>
          )}
        </>
      )}
      {runtime && (
        <section className={s.detail}>
          <h3>Legal worker</h3>
          <p>
            {runtime.companyReady ? "Company operations eligible" : "Company operations are gated"}
          </p>
          <p className={s.small}>{runtime.notice}</p>
          {runtime.tasks.length === 0 && <p>No worker heartbeat recorded yet.</p>}
          {runtime.tasks.map((task) => (
            <article className={s.event} key={task.task}>
              <strong>{task.task.replaceAll("_", " ")}</strong>
              <p>
                {task.stale ? "Heartbeat overdue" : "Recent heartbeat"} · {task.failure_count}{" "}
                failures
              </p>
              {task.last_error && <p>{task.last_error}</p>}
            </article>
          ))}
        </section>
      )}
    </div>
  );
}
