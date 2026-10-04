"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, Table, TableFrame, TextInput } from "@/components/ui/foundation";

import { useRouter } from "next/navigation";
import { useState } from "react";
import { workFetch } from "@/lib/work-fetch";
import { minorToDecimal } from "@/lib/finance-money";
import { RecordDocumentDrawer } from "@/components/attachments/record-document-drawer";
import { Card, FormLayout } from "@/components/ui/foundation";
type Row = Record<string, unknown>;
type Data = { schedules: Row[]; runs: Row[]; accounts: Row[] };
async function command(base: string, path: string, input: unknown) {
  const response = await workFetch(
    `${base.replace("/holdings/", "/api/v1/holdings/")}/finance/${path}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
      body: JSON.stringify(input),
    },
  );
  const body = (await response.json()) as { error?: { message: string } };
  if (!response.ok) throw new Error(body.error?.message ?? "Finance action failed.");
}
export function AccrualScheduleConsole({
  base,
  data,
  canWrite,
  canApprove,
}: {
  base: string;
  data: Data;
  canWrite: boolean;
  canApprove: boolean;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const run = async (path: string, input: unknown) => {
    setBusy(true);
    setError("");
    try {
      await command(base, path, input);
      router.refresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Finance action failed");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card as="section" appearance="standard" className="ui-panel" id="finance-accruals">
      <h2>Recurring accrual schedules</h2>
      <p className="muted">
        Prepare recurring expense recognition once, require independent approval, then post each due
        period into the open ledger period. Every run keeps its source schedule and amount evidence.
      </p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {canWrite && (
        <FormLayout
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void run("accrual-schedules", {
              description: f.get("description"),
              debitAccountId: f.get("debitAccountId"),
              creditAccountId: f.get("creditAccountId"),
              currency: f.get("currency"),
              total: f.get("total"),
              startDate: f.get("startDate"),
              endDate: f.get("endDate"),
              frequency: f.get("frequency"),
            });
          }}
        >
          <Field appearance="standard">
            Description
            <TextInput appearance="standard" name="description" required minLength={3} maxLength={240} />
          </Field>
          <Field appearance="standard">
            Debit account
            <Select appearance="standard" name="debitAccountId" required>
              <option value="">Select account</option>
              {data.accounts.map((account) => (
                <option key={String(account.id)} value={String(account.id)}>
                  {String(account.code)} · {String(account.name)} · {String(account.currency)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Credit account
            <Select appearance="standard" name="creditAccountId" required>
              <option value="">Select account</option>
              {data.accounts.map((account) => (
                <option key={String(account.id)} value={String(account.id)}>
                  {String(account.code)} · {String(account.name)} · {String(account.currency)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Currency
            <TextInput appearance="standard" name="currency" defaultValue="THB" minLength={3} maxLength={3} required />
          </Field>
          <Field appearance="standard">
            Total amount
            <TextInput appearance="standard" name="total" type="number" min="0.01" step="0.01" required />
          </Field>
          <Field appearance="standard">
            Start date
            <TextInput appearance="standard" name="startDate" type="date" required />
          </Field>
          <Field appearance="standard">
            End date
            <TextInput appearance="standard" name="endDate" type="date" required />
          </Field>
          <Field appearance="standard">
            Frequency
            <Select appearance="standard" name="frequency" defaultValue="monthly">
              <option value="monthly">Monthly</option>
              <option value="quarterly">Quarterly</option>
              <option value="annual">Annual</option>
            </Select>
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Save draft schedule
          </Button>
        </FormLayout>
      )}
      <TableFrame appearance="standard"
        label="Records"
        className="business-table-wrap"
        tabIndex={0}
        role="region"
        aria-label="Financial table — scroll for more columns"
      >
        <Table className="business-table">
          <thead>
            <tr>
              <th>Schedule</th>
              <th>Accounts</th>
              <th>Total</th>
              <th>Next period</th>
              <th>Status</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {data.schedules.map((schedule) => (
              <tr key={String(schedule.id)}>
                <td>
                  <strong>{String(schedule.description)}</strong>
                  <br />
                  <small>{String(schedule.frequency)}</small>
                </td>
                <td>
                  {String(schedule.debit_code)} → {String(schedule.credit_code)}
                  <br />
                  <small>{String(schedule.currency)}</small>
                </td>
                <td>{minorToDecimal(Number(schedule.total_minor))}</td>
                <td>{String(schedule.next_run_date)}</td>
                <td>{String(schedule.status)}</td>
                <td>
                  <RecordDocumentDrawer
                    base={base}
                    targetType="finance_accrual_schedule"
                    targetId={String(schedule.id)}
                    expectedTargetVersion={Number(schedule.version)}
                    editable={canWrite}
                    title={`${String(schedule.description)} · accrual support`}
                    triggerLabel="Documents"
                    purposes={[
                      {
                        key: "accrual_support",
                        label: "Accrual support",
                        description:
                          "Attach the calculation source, contract, or approved schedule supporting this accrual revision.",
                      },
                    ]}
                  />
                  {schedule.status === "draft" && canApprove && (
                    <Button appearance="standard"
                      type="submit"
                      variant="secondary"
                      disabled={busy}
                      onClick={() =>
                        run(`accrual-schedules/${String(schedule.id)}/approve`, {
                          expectedVersion: Number(schedule.version),
                          reason: "Independent Finance approval",
                        })
                      }
                    >
                      Approve
                    </Button>
                  )}
                  {["draft", "active"].includes(String(schedule.status)) && canWrite && (
                    <Button appearance="standard"
                      type="submit"
                      variant="secondary"
                      disabled={busy}
                      onClick={() =>
                        run(`accrual-schedules/${String(schedule.id)}/cancel`, {
                          expectedVersion: Number(schedule.version),
                          reason: "Schedule cancelled by Finance",
                        })
                      }
                    >
                      Cancel
                    </Button>
                  )}
                  {schedule.status === "active" && canWrite && (
                    <Button appearance="standard"
                      type="submit"
                      variant="secondary"
                      disabled={busy}
                      onClick={() => run(`accrual-schedules/${String(schedule.id)}/prepare`, {})}
                    >
                      Prepare due period
                    </Button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      </TableFrame>
      {!data.schedules.length && <p>No recurring accrual schedules.</p>}
      <h3>Prepared and posted periods</h3>
      <TableFrame appearance="standard"
        label="Records"
        className="business-table-wrap"
        tabIndex={0}
        role="region"
        aria-label="Financial table — scroll for more columns"
      >
        <Table className="business-table">
          <thead>
            <tr>
              <th>Schedule</th>
              <th>Period</th>
              <th>Amount</th>
              <th>Status</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            {data.runs.map((item) => (
              <tr key={String(item.id)}>
                <td>{String(item.description)}</td>
                <td>
                  {String(item.period_start)} → {String(item.period_end)}
                </td>
                <td>
                  {String(item.currency)} {minorToDecimal(Number(item.amount_minor))}
                </td>
                <td>{String(item.status)}</td>
                <td>
                  {item.status === "prepared" && canApprove && (
                    <Button appearance="standard"
                      type="submit"
                      variant="primary"
                      disabled={busy}
                      onClick={() =>
                        run(`accrual-runs/${String(item.id)}/post`, {
                          expectedVersion: Number(item.version),
                          reason: "Post recurring accrual after independent review",
                        })
                      }
                    >
                      Post journal
                    </Button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      </TableFrame>
      {!data.runs.length && <p>No accrual periods prepared yet.</p>}
    </Card>
  );
}
