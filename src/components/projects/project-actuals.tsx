"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useState, type FormEvent } from "react";
import {useProjectRecordCommand} from "./use-project-record-command";
import {ProjectRecordRecovery} from "./project-record-recovery";
import type {ProjectRecordOwner} from "@/lib/projects/project-record-command";
import {useProjectPurchases} from "./purchase-context";
import {projectPurchased} from "@/lib/projects/purchase-controls";
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
  ownerContext,
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
  ownerContext?:ProjectRecordOwner;
}) {
  const purchases=useProjectPurchases();
  const timeNew=projectPurchased(purchases,"projects.time","newWork"),timeSettle=projectPurchased(purchases,"projects.time","settleExisting"),costNew=projectPurchased(purchases,"projects.costs","newWork"),costSettle=projectPurchased(purchases,"projects.costs","settleExisting"),readCosts=canReadCosts&&projectPurchased(purchases,"projects.costs","history");
  const [workDate, setWorkDate] = useState("");
  const [minutes, setMinutes] = useState("60");
  const [sourceLineId, setSourceLineId] = useState("");
  const [amount, setAmount] = useState("");
  const [timeRevision,setTimeRevision]=useState(0),[costRevision,setCostRevision]=useState(0),[timeDirty,setTimeDirty]=useState(false),[costDirty,setCostDirty]=useState(false);
  const timeCommand=useProjectRecordCommand({kind:'time',base,projectId,owner:ownerContext,workLogs,editable,purchases,draftVersion:timeRevision,dirty:timeDirty}),costCommand=useProjectRecordCommand({kind:'costs',base,projectId,owner:ownerContext,costs,costSources,editable,canReadCosts,canWriteCosts,canApproveCosts,purchases,draftVersion:costRevision,dirty:costDirty});
  const {send,busy:timeBusy,error:timeError,pending:timePending}=timeCommand,{busy:costBusy,error:costError,pending:costPending}=costCommand;
  const owner = isProjectOwner;
  const selectedWorkLog = workLogs.find((log) => log.work_date === workDate && log.is_actor);
  const selectedRejectedCost = costs.find(
    (row) => row.journal_line_id === sourceLineId && row.state === "rejected" && row.is_creator,
  );

  async function submitWorkLog(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if(!timeNew||!editable||timeBusy||timePending)return;
    const rejected = workLogs.find(
      (log) => log.work_date === workDate && log.state === "rejected" && log.is_actor,
    );
    await send(`projects/${encodeURIComponent(projectId)}/work-logs`, {
      workDate,
      minutes: Number(minutes),
      ...(rejected ? { expectedVersion: rejected.version } : {}),
    },event.currentTarget,{saved:()=>{setWorkDate("");setTimeDirty(false);}});
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

  async function sendCost(path:string,input:unknown,form?:HTMLFormElement){return costCommand.send(`finance/${path}`,input,form);}

  async function submitCost(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if(!costNew||!editable||!canWriteCosts||costBusy||costPending)return;
    await costCommand.send("finance/project-cost-allocations", {
      projectId,
      sourceLineId,
      amount,
      ...(selectedRejectedCost ? { expectedVersion: selectedRejectedCost.version } : {}),
    },event.currentTarget,{saved:()=>{setSourceLineId("");setAmount("");setCostDirty(false);}});
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
                {editable&&timeSettle&&log.can_review && log.state === "submitted" && (
                  <div className="ui-form-actions">
                    <Button appearance="standard"
                      variant="secondary"
                      type="button"
                      disabled={timeBusy||timePending}
                      onClick={() => void reviewWorkLog(log, "approve")}
                    >
                      Approve
                    </Button>
                    <Button appearance="standard"
                      variant="ghost"
                      type="button"
                      disabled={timeBusy||timePending}
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
        {((editable&&timeNew)||timeDirty||timePending)&&<FormLayout className="ui-form-grid" data-work-dirty={timeDirty?"true":undefined} onChangeCapture={()=>{setTimeRevision(value=>value+1);setTimeDirty(true);}} onSubmit={submitWorkLog}>
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
              timeBusy || timePending || !editable || !timeNew || !workDate || (!!selectedWorkLog && selectedWorkLog.state !== "rejected")
            }
          >
            Submit project hours
          </Button>
          {selectedWorkLog && selectedWorkLog.state !== "rejected" && (
            <p>This project already has a time entry for that date.</p>
          )}
        </FormLayout>}
        <ProjectRecordRecovery pending={timePending} busy={timeBusy} retry={timeCommand.retry}/>
        {timeCommand.notice&&<InlineNotice role="status">{timeCommand.notice}</InlineNotice>}
        {timeError && <InlineNotice tone="danger" role="alert">{timeError}</InlineNotice>}
      </Card>

      <Card as="section" appearance="standard" className="ui-panel" aria-labelledby="project-cost-heading">
        <h2 id="project-cost-heading">Posted project costs</h2>
        {!readCosts ? (
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
                    {editable&&costSettle&&canApproveCosts && row.can_review && row.state === "draft" && (
                      <div className="ui-form-actions">
                        <Button appearance="standard"
                          variant="secondary"
                          type="button"
                          disabled={costBusy||costPending}
                          onClick={() => void reviewCost(row, "approve")}
                        >
                          Approve
                        </Button>
                        <Button appearance="standard"
                          variant="ghost"
                          type="button"
                          disabled={costBusy||costPending}
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
            {costError && <InlineNotice tone="danger" role="alert">{costError}</InlineNotice>}
          </>
        )}
            {((readCosts&&editable&&costNew&&canWriteCosts)||costDirty||costPending) && (
              <FormLayout className="ui-form-grid" data-work-dirty={costDirty?"true":undefined} onChangeCapture={()=>{setCostRevision(value=>value+1);setCostDirty(true);}} onSubmit={submitCost}>
                <Field appearance="standard">
                  Posted expense source
                  <Select appearance="standard"
                    required
                    value={sourceLineId}
                    onChange={(event) => setSourceLineId(event.target.value)}
                  >
                    <option value="">Choose a posted expense line</option>
                    {(readCosts?costSources:[]).map((source) => (
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
                <Button appearance="standard" variant="primary" clientManaged type="submit" disabled={costBusy || costPending || !editable || !costNew || !canWriteCosts || !sourceLineId}>
                  Submit cost allocation
                </Button>
                {!costSources.length && (
                  <p>No eligible posted expense lines remain in this scope.</p>
                )}
              </FormLayout>
            )}
        <ProjectRecordRecovery pending={costPending} busy={costBusy} retry={costCommand.retry}/>
        {costCommand.notice&&<InlineNotice role="status">{costCommand.notice}</InlineNotice>}
        {!readCosts&&costError&&<InlineNotice tone="danger" role="alert">{costError}</InlineNotice>}
      </Card>
    </Stack>
  );
}
