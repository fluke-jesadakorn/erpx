"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { RecordLink } from "@/components/records/record-link";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Table, TableFrame, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { LedgerRow } from "@/server/modules/finance/public/ledger";
import { Card } from "@/components/ui/foundation";

export function JournalBatchReview({
  base,
  journals,
  periods,
  userId,
}: {
  base: string;
  journals: LedgerRow[];
  periods: LedgerRow[];
  userId: string;
}) {
  const candidates = journals.filter(
    (j) =>
      ["draft", "in_review"].includes(String(j.status)) &&
      ["manual", "opening_import"].includes(String(j.source_type)) &&
      j.created_by !== userId,
  );
  const [selected, setSelected] = useState<string[]>([]),
    [reason, setReason] = useState(""),
    [pending, setPending] = useState(false),
    [error, setError] = useState("");
  const attempt = useRef<{ fingerprint: string; key: string } | null>(null),
    router = useRouter();
  const rows = candidates.filter((j) => selected.includes(String(j.id))),
    scope = new Set(rows.map((j) => `${j.chart_id}:${j.period_id}:${j.currency}`));
  const count = rows.reduce((total, j) => total + Number(j.line_count ?? 0), 0);
  async function post() {
    const input = {
        journals: rows.map((j) => ({ id: String(j.id), expectedVersion: Number(j.version) })),
        reason,
      },
      fingerprint = JSON.stringify(input);
    if (attempt.current?.fingerprint !== fingerprint)
      attempt.current = { fingerprint, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const response = await workFetch(
        `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/journals/batch-post`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body: fingerprint,
        },
      );
      const result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? "Could not post journals.");
      attempt.current = null;
      setSelected([]);
      setReason("");
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Could not post journals.");
    } finally {
      setPending(false);
    }
  }
  if (!candidates.length) return null;
  return (
    <Card as="section" appearance="standard" className="ui-panel" aria-label="Batch posting review">
      <h2>Batch posting review</h2>
      <p className="muted">
        Select reviewed drafts in one chart, period and currency. Up to 100 drafts / 1,000 lines
        post atomically.
      </p>
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
              <th>Select</th>
              <th>Journal number</th>
              <th>Date</th>
              <th>Period</th>
              <th>Debit</th>
              <th>Credit</th>
              <th>Lines</th>
            </tr>
          </thead>
          <tbody>
            {candidates.map((j) => (
              <tr key={String(j.id)}>
                <td>
                  <TextInput appearance="standard"
                    type="checkbox"
                    aria-label={`Select ${j.journal_number}`}
                    checked={selected.includes(String(j.id))}
                    disabled={pending}
                    onChange={(e) =>
                      setSelected(
                        e.target.checked
                          ? [...selected, String(j.id)]
                          : selected.filter((id) => id !== j.id),
                      )
                    }
                  />
                </td>
                <td>
                  <RecordLink href={canonicalUiHref(`${base}/finance/journals/${encodeURIComponent(String(j.id))}`)}>{String(j.journal_number)}</RecordLink>
                </td>
                <td>{String(j.entry_date)}</td>
                <td>
                  {String(periods.find((p) => p.id === j.period_id)?.name ?? "Accounting period")} ·{" "}
                  {String(j.currency)}
                </td>
                <td>{minorToDecimal(Number(j.debit))}</td>
                <td>{minorToDecimal(Number(j.credit))}</td>
                <td>{Number(j.line_count)}</td>
              </tr>
            ))}
          </tbody>
        </Table>
      </TableFrame>
      <p>
        {rows.length} Drafts · {count} Lines
      </p>
      {scope.size > 1 && (
        <InlineNotice tone="danger" role="alert" className="error">
          Select one period and currency.
        </InlineNotice>
      )}
      <Field appearance="standard">
        Review evidence and reason
        <TextArea appearance="standard"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          required
          minLength={10}
          maxLength={2000}
        />
      </Field>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      <Button appearance="standard"
        type="submit"
        variant="primary"
        disabled={
          pending ||
          !rows.length ||
          rows.length > 100 ||
          count > 1000 ||
          scope.size !== 1 ||
          reason.trim().length < 10
        }
        onClick={() => void post()}
      >
        {pending ? "Posting…" : "Confirm review and post batch"}
      </Button>
    </Card>
  );
}
