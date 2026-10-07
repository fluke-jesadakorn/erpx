"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useRef, useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { useStaffCommand } from "./staff-work-controls";
import type {
  ProjectCostAllocationRow,
  ProjectCostSourceRow,
  ProjectWorkLogRow,
} from "@/server/modules/projects/public/project-actuals";
import { Stack, Card, FormLayout } from "@/components/ui/foundation";

export function ProjectActualsConsole({
  base,
  projectId,
  isProjectOwner,
  workLogs,
  costs,
  costSources,
  canReadCosts,
  canWriteCosts,
  canApproveCosts,
  editable=true,
}: {
  base: string;
  projectId: string;
  isProjectOwner: boolean;
  workLogs: ProjectWorkLogRow[];
  costs: ProjectCostAllocationRow[];
  costSources: ProjectCostSourceRow[];
  canReadCosts: boolean;
  canWriteCosts: boolean;
  canApproveCosts: boolean;
  editable?:boolean;
}) {
  const { send, busy: timeBusy, error: timeError } = useStaffCommand(base);
  const router = useRouter();
  const [workDate, setWorkDate] = useState("");
  const [minutes, setMinutes] = useState("60");
  const [sourceLineId, setSourceLineId] = useState("");
  const [amount, setAmount] = useState("");
  const [costBusy, setCostBusy] = useState(false);
  const [costError, setCostError] = useState("");
  const attempt = useRef<{ signature: string; key: string } | null>(null);
  const owner = isProjectOwner;
  const selectedWorkLog = workLogs.find((log) => log.work_date === workDate && log.is_actor);
  const selectedRejectedCost = costs.find(
    (row) => row.journal_line_id === sourceLineId && row.state === "rejected" && row.is_creator,
  );

  async function submitWorkLog(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const rejected = workLogs.find(
      (log) => log.work_date === workDate && log.state === "rejected" && log.is_actor,
    );
    const result = await send(`projects/${encodeURIComponent(projectId)}/work-logs`, {
      workDate,
      minutes: Number(minutes),
      ...(rejected ? { expectedVersion: rejected.version } : {}),
    });
    if (result) setWorkDate("");
  }

  async function reviewWorkLog(log: ProjectWorkLogRow, action: "approve" | "reject") {
    await send(
      `projects/${encodeURIComponent(projectId)}/work-logs/${encodeURIComponent(log.id)}/review`,
      {
        action,
        expectedVersion: log.version,
        reason:
          action === "approve"
            ? "Project time reviewed and approved by the project owner."
            : "Please correct the work date or minutes and resubmit this time entry.",
      },
    );
  }

  async function sendCost(path: string, input: unknown) {
    const body = JSON.stringify(input),
      signature = `${path}${body}`;
    if (attempt.current?.signature !== signature)
      attempt.current = { signature, key: crypto.randomUUID() };
    setCostBusy(true);
    setCostError("");
    try {
      const response = await workFetch(
        `${base.replace("/holdings/", "/api/v1/holdings/")}/finance/${path}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body,
        },
      );
      const result = (await response.json()) as { error?: { message?: string } };
      if (!response.ok)
        throw new Error(result.error?.message ?? "Unable to save this cost allocation.");
      attempt.current = null;
      router.refresh();
      return true;
    } catch (error) {
      setCostError(error instanceof Error ? error.message : "Unable to save this cost allocation.");
      return false;
    } finally {
      setCostBusy(false);
    }
  }

  async function submitCost(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const saved = await sendCost("project-cost-allocations", {
      projectId,
      sourceLineId,
      amount,
      ...(selectedRejectedCost ? { expectedVersion: selectedRejectedCost.version } : {}),
    });
    if (saved) {
      setSourceLineId("");
      setAmount("");
    }
  }

  async function reviewCost(row: ProjectCostAllocationRow, action: "approve" | "reject") {
    await sendCost(`project-cost-allocations/${encodeURIComponent(row.id)}/review`, {
      action,
      expectedVersion: row.version,
      reason:
        action === "approve"
          ? "Posted expense allocation reviewed and approved by Finance."
          : "Please correct this project cost allocation and resubmit it.",
    });
  }

  return (
    <Stack as="div" className="ui-stack" id="project-actuals">
      <Card as="section" appearance="standard" className="ui-panel" aria-labelledby="project-hours-heading">
        <h2 id="project-hours-heading">Tracked project hours</h2>
        <p>
          Only owner-approved entries count as actual project time. Entries are private to the
          contributor and project owner.
        </p>
        {workLogs.length ? (
          <ul className="ui-stack">
            {workLogs.map((log) => (
              <li className="ui-record-item" key={log.id}>
                <div>
                  <strong>
                    {log.work_date} · {log.minutes} minutes
                  </strong>
                  <p>
                    {log.state.replaceAll("_", " ")}
                    {log.state === "rejected" && log.rejection_reason
                      ? ` · ${log.rejection_reason}`
                      : ""}
                    {owner && !log.is_actor ? ` · ${log.user_id}` : ""}
                  </p>
                </div>
                {log.can_review && log.state === "submitted" && (
                  <div className="ui-form-actions">
                    <Button appearance="standard"
                      variant="secondary"
                      type="button"
                      disabled={timeBusy}
                      onClick={() => void reviewWorkLog(log, "approve")}
                    >
                      Approve
                    </Button>
                    <Button appearance="standard"
                      variant="ghost"
                      type="button"
                      disabled={timeBusy}
                      onClick={() => void reviewWorkLog(log, "reject")}
                    >
                      Return
                    </Button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        ) : (
          <p>No project time has been submitted.</p>
        )}
        {editable&&<FormLayout className="ui-form-grid" onSubmit={submitWorkLog}>
          <Field appearance="standard">
            Work date
            <TextInput appearance="standard"
              required
              type="date"
              value={workDate}
              onChange={(event) => setWorkDate(event.target.value)}
            />
          </Field>
          <Field appearance="standard">
            Minutes worked
            <TextInput appearance="standard"
              required
              type="number"
              min={1}
              max={1440}
              step={1}
              value={minutes}
              onChange={(event) => setMinutes(event.target.value)}
            />
          </Field>
          <Button appearance="standard"
            variant="primary"
            clientManaged type="submit"
            disabled={
              timeBusy || !workDate || (!!selectedWorkLog && selectedWorkLog.state !== "rejected")
            }
          >
            Submit project hours
          </Button>
          {selectedWorkLog && selectedWorkLog.state !== "rejected" && (
            <p>This project already has a time entry for that date.</p>
          )}
        </FormLayout>}
        {timeError && <InlineNotice tone="danger" role="alert">{timeError}</InlineNotice>}
      </Card>

      <Card as="section" appearance="standard" className="ui-panel" aria-labelledby="project-cost-heading">
        <h2 id="project-cost-heading">Posted project costs</h2>
        {!canReadCosts ? (
          <p>Finance permission and project membership are required to view cost actuals.</p>
        ) : (
          <>
            <p>
              Costs come from Finance-approved allocations of posted expense journal lines. Reversed
              journals are excluded.
            </p>
            {costs.length ? (
              <ul className="ui-stack">
                {costs.map((row) => (
                  <li className="ui-record-item" key={row.id}>
                    <div>
                      <strong>
                        {row.journal_number} · {row.currency} {row.amount}
                      </strong>
                      <p>
                        {row.entry_date} · {row.account_code} {row.account_name} · {row.state}
                        {row.journal_status !== "posted" ? ` · Source ${row.journal_status}` : ""}
                      </p>
                    </div>
                    {canApproveCosts && row.can_review && row.state === "draft" && (
                      <div className="ui-form-actions">
                        <Button appearance="standard"
                          variant="secondary"
                          type="button"
                          disabled={costBusy}
                          onClick={() => void reviewCost(row, "approve")}
                        >
                          Approve
                        </Button>
                        <Button appearance="standard"
                          variant="ghost"
                          type="button"
                          disabled={costBusy}
                          onClick={() => void reviewCost(row, "reject")}
                        >
                          Return
                        </Button>
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            ) : (
              <p>No project cost allocations have been recorded.</p>
            )}
            {canWriteCosts && (
              <FormLayout className="ui-form-grid" onSubmit={submitCost}>
                <Field appearance="standard">
                  Posted expense source
                  <Select appearance="standard"
                    required
                    value={sourceLineId}
                    onChange={(event) => setSourceLineId(event.target.value)}
                  >
                    <option value="">Choose a posted expense line</option>
                    {costSources.map((source) => (
                      <option key={source.source_line_id} value={source.source_line_id}>
                        {source.journal_number} · {source.entry_date} · {source.account_code} ·{" "}
                        {source.currency} {source.remaining} available
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Allocated amount
                  <TextInput appearance="standard"
                    required
                    inputMode="decimal"
                    pattern="\d+(\.\d{1,2})?"
                    value={amount}
                    onChange={(event) => setAmount(event.target.value)}
                    placeholder="0.00"
                  />
                </Field>
                <Button appearance="standard" variant="primary" clientManaged type="submit" disabled={costBusy || !sourceLineId}>
                  Submit cost allocation
                </Button>
                {!costSources.length && (
                  <p>No eligible posted expense lines remain in this scope.</p>
                )}
              </FormLayout>
            )}
            {costError && <InlineNotice tone="danger" role="alert">{costError}</InlineNotice>}
          </>
        )}
      </Card>
    </Stack>
  );
}
