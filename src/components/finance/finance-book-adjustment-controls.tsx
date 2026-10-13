"use client";
import {FinanceEvidenceDocuments} from "./finance-evidence-documents";
import { useState } from "react";
import { useRouter } from "next/navigation";
import { financeCommandFetch as workFetch } from "@/lib/finance/finance-command-fetch";
import { financeCsvRows } from "@/lib/finance/finance-csv";
import {
  Button,
  Card,
  Field,
  FormLayout,
  InlineNotice,
  Select,
  TextArea,
  TextInput,
} from "@/components/ui/foundation";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { RecordLink } from "@/components/records/record-link";
type Row = Record<string, string | number | null>;
export function FinanceBookAdjustmentControls({
  base,
  data,
  canWrite,
  canApprove,
  userId,
}: {
  base: string;
  data: {
    rows: Row[];
    books: Row[];
    accounts: Row[];
    sources: Row[];
    periods: Row[];
  };
  canWrite: boolean;
  canApprove: boolean;
  userId: string;
}) {
  const router = useRouter(),
    [book, setBook] = useState(String(data.books[0]?.id ?? "")),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function run(path: string, input: unknown) {
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(
          `${base.replace("/holdings/", "/api/v1/holdings/")}/finance/book-adjustments${path}`,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Idempotency-Key": crypto.randomUUID(),
            },
            body: JSON.stringify(input),
          },
        ),
        body = await r.json();
      if (!r.ok)
        throw new Error(body.error?.message ?? "Book adjustment failed");
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Book adjustment failed");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card appearance="standard" as="section" className="ui-panel">
      <h2>Secondary book policy adjustments</h2>
      <p className="muted">
        Retain a policy difference against its business source. These
        adjustments use noncontrol accounts and affect only the selected
        secondary book.
      </p>
      {error && <InlineNotice tone="danger">{error}</InlineNotice>}
      {canWrite && data.books.length > 0 && (
        <FormDisclosure error={error}
          appearance="standard"
          drawerTitle="Prepare secondary book adjustment"
        >
          <summary>Prepare adjustment</summary>
          <FormLayout
            className="ui-form-grid"
            onSubmit={async (e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              try {
                const dimensions = Object.fromEntries(
                    String(f.get("dimensions") ?? "")
                      .split("\n")
                      .filter(Boolean)
                      .map((l) => l.split("=")),
                  ),
                  lines = financeCsvRows(String(f.get("lines"))).map((l) => ({
                    accountId: String(
                      data.accounts.find(
                        (a) => a.chart_id === book && a.code === l.accountCode,
                      )?.id ?? "",
                    ),
                    debit: l.debit,
                    credit: l.credit,
                    dimensions,
                    description: l.description,
                  }));
                await run("", {
                  chartId: book,
                  primaryJournalId: f.get("source"),
                  periodId: f.get("period"),
                  entryDate: f.get("date"),
                  reason: f.get("reason"),
                  lines,
                });
              } catch (error) {
                setError(
                  error instanceof Error
                    ? error.message
                    : "Invalid adjustment CSV",
                );
              }
            }}
          >
            <Field appearance="standard">
              Secondary book
              <Select
                appearance="standard"
                value={book}
                onChange={(e) => setBook(e.target.value)}
              >
                {data.books.map((b) => (
                  <option key={String(b.id)} value={String(b.id)}>
                    {String(b.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Posted business source
              <Select appearance="standard" name="source" required>
                <option value="">Select source</option>
                {data.sources
                  .filter((s) => s.chart_id === book)
                  .map((s) => (
                    <option key={String(s.id)} value={String(s.id)}>
                      {String(s.entry_date)} · {String(s.journal_number)} ·{" "}
                      {String(s.memo ?? "")}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Open period
              <Select appearance="standard" name="period" required>
                {data.periods.map((p) => (
                  <option key={String(p.id)} value={String(p.id)}>
                    {String(p.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Accounting date
              <TextInput
                appearance="standard"
                type="date"
                name="date"
                required
              />
            </Field>
            <Field appearance="standard">
              Adjustment lines CSV
              <TextArea
                appearance="standard"
                name="lines"
                required
                placeholder={
                  "accountCode,debit,credit,description\n5000,10.01,0,Policy adjustment\n1500,0,10.01,Policy adjustment"
                }
              />
            </Field>
            <Field appearance="standard">
              Dimensions, one code=value per line
              <TextArea appearance="standard" name="dimensions" />
            </Field>
            <Field appearance="standard">
              Policy evidence and reason
              <TextArea
                appearance="standard"
                name="reason"
                required
                minLength={3}
              />
            </Field>
            <Button
              appearance="standard"
              clientManaged
              type="submit"
              disabled={busy}
            >
              Prepare for approval
            </Button>
          </FormLayout>
        </FormDisclosure>
      )}
      {data.rows.map((r) => (
        <Card
          appearance="standard"
          as="section"
          key={String(r.id)}
          className="ui-panel"
        >
          <h3>
            {String(r.reason)} · {String(r.status)}
          </h3>
          <p>
            Book{" "}
            {String(
              data.books.find((b) => b.id === r.chart_id)?.name ?? r.chart_id,
            )}
          </p>
          <RecordLink href={`${base}/finance/journals/${r.primary_journal_id}`}>
            Business source
          </RecordLink>
          {r.journal_id && (
            <RecordLink href={`${base}/finance/journals/${r.journal_id}`}>
              Adjustment journal
            </RecordLink>
          )}
          {r.reversal_journal_id && (
            <RecordLink
              href={`${base}/finance/journals/${r.reversal_journal_id}`}
            >
              Reversal journal
            </RecordLink>
          )}
          <details>
            <summary>Retained evidence</summary>
            <pre>
              {JSON.stringify(JSON.parse(String(r.content_json)), null, 2)}
            </pre>
            <p>{String(r.source_hash)}</p>
          </details>
          <FinanceEvidenceDocuments base={base} row={r} targetType="finance_book_adjustment" canWrite={canWrite}/>
          {canApprove && r.prepared_by !== userId && r.status === "draft" && (
            <FormLayout
              className="ui-form-grid"
              onSubmit={async (e) => {
                e.preventDefault();
                const f = new FormData(
                  e.currentTarget,
                  (e.nativeEvent as SubmitEvent).submitter,
                );
                await run(`/${r.id}/${f.get("action")}`, {
                  expectedVersion: Number(r.version),
                  reason: f.get("reason"),
                });
              }}
            >
              <Field appearance="standard">
                Review reason
                <TextInput
                  appearance="standard"
                  name="reason"
                  required
                  minLength={3}
                />
              </Field>
              {["post", "reject"].map((a) => (
                <Button
                  appearance="standard"
                  clientManaged
                  type="submit"
                  name="action"
                  value={a}
                  disabled={busy}
                  key={a}
                >
                  {a}
                </Button>
              ))}
            </FormLayout>
          )}
          {canApprove && r.prepared_by !== userId && r.status === "posted" && (
            <FormDisclosure error={error}
              appearance="standard"
              drawerTitle="Reverse secondary book adjustment"
            >
              <summary>Reverse adjustment</summary>
              <FormLayout
                className="ui-form-grid"
                onSubmit={async (e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget);
                  await run(`/${r.id}/reverse`, {
                    expectedVersion: Number(r.version),
                    periodId: f.get("period"),
                    entryDate: f.get("date"),
                    reason: f.get("reason"),
                  });
                }}
              >
                <Field appearance="standard">
                  Open period
                  <Select appearance="standard" name="period" required>
                    {data.periods.map((p) => (
                      <option key={String(p.id)} value={String(p.id)}>
                        {String(p.name)}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Reversal date
                  <TextInput
                    appearance="standard"
                    name="date"
                    type="date"
                    required
                  />
                </Field>
                <Field appearance="standard">
                  Correction reason
                  <TextArea
                    appearance="standard"
                    name="reason"
                    required
                    minLength={3}
                  />
                </Field>
                <Button
                  appearance="standard"
                  clientManaged
                  type="submit"
                  disabled={busy}
                >
                  Reverse with retained history
                </Button>
              </FormLayout>
            </FormDisclosure>
          )}
        </Card>
      ))}
    </Card>
  );
}
