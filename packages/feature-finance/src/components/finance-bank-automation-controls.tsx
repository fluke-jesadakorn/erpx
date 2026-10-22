"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { FinanceEvidenceDocuments } from "@erpx/feature-finance/components/finance-evidence-documents";
import { useState, useContext } from "react";
import { EmbeddedRecord } from "@erpx/ui/components/record-context";
import { recordCanonicalHref } from "@erpx/feature-records/lib/record-drawer";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {useFinanceCommand} from "@erpx/feature-finance/components/use-finance-command";
import {captureFinanceForm} from "@erpx/feature-finance/components/finance-draft-boundary";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import {
  Button,
  Card,
  Field,
  FormLayout,
  InlineNotice,
  Select,
  TextArea,
  TextInput,
  Table,
  TableFrame,
} from "@erpx/ui/components/foundation";
import { RecordCatalog } from "@erpx/feature-records/components/record-catalog";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { RecordLink } from "@erpx/feature-records/components/record-link";
import { ListPagination } from "@erpx/ui/components/list-pagination";
type Row = Record<string, unknown>;
type Data = {
  mappings: Row[];
  groups: Row[];
  suggestions: {
    bankTransactionIds: string[];
    journalLineIds: string[];
    confidence: string;
    amount: string;
  }[];
  rows: Row[];
  targets: Row[];
  suggestionTargets?: Row[];
  pages?: Record<
    "bank" | "cash" | "groups",
    { nextCursor: string | null; previousCursor: string | null }
  >;
};
export function FinanceBankAutomationControls({
  base,
  data,
  accounts,
  canWrite,
  canApprove,
  userId,
  query = {},
}: {
  base: string;
  data: Data;
  accounts: Row[];
  canWrite: boolean;
  canApprove: boolean;
  userId: string;
  query?: Record<string, string | undefined>;
}) {
  const recordOnly = useContext(EmbeddedRecord),router=useRouter();
  const {send,busy,error,draftScope,canPrepare,canSettle}=useFinanceCommand(base,"finance.banking");
  const [selectedBanks, setSelectedBanks] = useState<Row[]>([]);
  const [selectedCash, setSelectedCash] = useState<Row[]>([]);
  async function run(path:string,input:unknown){
    const completion=captureFinanceForm();
    const result=await send(path,input,path==="bank-csv-mappings"?"new-work":"settle-existing");
    if(result&&path==="bank-match-groups"&&completion?.form.hasAttribute("data-bank-match-selection")&&completion.formEditVersion===completion.form.getAttribute("data-erpx-edit-version")){
      setSelectedBanks([]);setSelectedCash([]);
    }
  }
  return (
    <Card appearance="standard" as="section" className="ui-panel">
      <h2>CSV mappings and match review</h2>
      <p className="muted">
        Reuse your bank CSV columns. Amount and date suggestions prepare a
        proposal for independent review. Matching leaves the posted ledger
        amounts intact.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert">
          {error}
        </InlineNotice>
      )}
      {canWrite && !recordOnly && (
        <FormDisclosure error={error}
          appearance="standard"
          drawerTitle="Save bank CSV columns"
        >
          <summary>Save CSV mapping</summary>
          <FormLayout data-finance-draft={draftScope}
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void run("bank-csv-mappings", {
                name: f.get("name"),
                dateFormat: f.get("dateFormat"),
                columns: {
                  externalId: f.get("externalId"),
                  transactionDate: f.get("transactionDate"),
                  amount: f.get("amount"),
                  description: f.get("description") || undefined,
                  currency: f.get("currency") || undefined,
                },
              });
            }}
          >
            {[
              ["name", "Mapping name"],
              ["externalId", "Reference column"],
              ["transactionDate", "Date column"],
              ["amount", "Signed amount column"],
              ["description", "Description column (optional)"],
              ["currency", "Currency column (optional)"],
            ].map(([name, label], i) => (
              <Field appearance="standard" key={name}>
                {label}
                <TextInput appearance="standard" name={name} required={i < 4} />
              </Field>
            ))}
            <Field appearance="standard">
              Date format
              <Select appearance="standard" name="dateFormat">
                <option value="YYYY-MM-DD">YYYY-MM-DD</option>
                <option value="DD/MM/YYYY">DD/MM/YYYY</option>
              </Select>
            </Field>
            <Button
              appearance="standard"
              clientManaged
              type="submit"
              disabled={!canPrepare || busy}
            >
              Save mapping
            </Button>
          </FormLayout>
        </FormDisclosure>
      )}
      {!recordOnly && (
        <RecordCatalog
          base={base}
          kind="bank_profile"
          title="Bank CSV mappings"
          rows={data.mappings}
          label="name"
        />
      )}
      {canWrite && data.mappings.length > 0 && (
        <FormDisclosure error={error}
          appearance="standard"
          drawerTitle="Import mapped bank CSV"
        >
          <summary>Import with saved mapping</summary>
          <FormLayout data-finance-draft={draftScope}
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void run("bank-csv-imports", {
                mappingId: f.get("mappingId"),
                bankAccountId: f.get("bankAccountId"),
                statementNumber: f.get("statementNumber"),
                statementDate: f.get("statementDate"),
                opening: f.get("opening"),
                closing: f.get("closing"),
                csv: f.get("csv"),
              });
            }}
          >
            <Field appearance="standard">
              Saved mapping
              <Select appearance="standard" name="mappingId" required>
                {data.mappings.map((m) => (
                  <option key={String(m.id)} value={String(m.id)}>
                    {String(m.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Bank account
              <Select appearance="standard" name="bankAccountId" required>
                {accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.code)} · {String(a.name)} · {String(a.currency)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Statement reference
              <TextInput
                appearance="standard"
                name="statementNumber"
                required
              />
            </Field>
            <Field appearance="standard">
              Statement date
              <TextInput
                appearance="standard"
                name="statementDate"
                type="date"
                required
              />
            </Field>
            <Field appearance="standard">
              Opening balance
              <TextInput appearance="standard" name="opening" required />
            </Field>
            <Field appearance="standard">
              Closing balance
              <TextInput appearance="standard" name="closing" required />
            </Field>
            <Field appearance="standard">
              CSV, including header (maximum 500 movements)
              <TextArea appearance="standard" name="csv" rows={8} required />
            </Field>
            <Button
              appearance="standard"
              clientManaged
              type="submit"
              disabled={!canSettle || busy}
            >
              Import statement
            </Button>
          </FormLayout>
        </FormDisclosure>
      )}
      <TableFrame appearance="standard" label="Bank match suggestions">
        <Table>
          <thead>
            <tr>
              <th>Bank movement</th>
              <th>Suggestion / exception</th>
              <th>Prepare review</th>
            </tr>
          </thead>
          <tbody>
            {data.suggestions.map((s) => (
              <tr key={s.bankTransactionIds.join(",")}>
                <td>
                  {String(
                    data.rows.find((r) => r.id === s.bankTransactionIds[0])
                      ?.transaction_date,
                  )}{" "}
                  · {s.amount}
                </td>
                <td>
                  {s.confidence === "needs_review" ? (
                    "No unique matching amount; choose posted cash lines below."
                  ) : (
                    <ul>
                      {s.journalLineIds.map((id) => {
                        const line = (
                          data.suggestionTargets ?? data.targets
                        ).find((r) => r.id === id);
                        return (
                          <li key={id}>
                            <RecordLink
                              href={canonicalUiHref(`${base}/finance/journals/${line?.journal_id}`)}
                            >
                              {String(line?.entry_date)} ·{" "}
                              {minorToDecimal(
                                Number(line?.debit_minor) -
                                  Number(line?.credit_minor),
                              )}
                            </RecordLink>
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </td>
                <td>
                  {canWrite && s.journalLineIds.length > 0 && (
                    <Button
                      appearance="standard"
                      clientManaged
                      disabled={!canSettle || busy}
                      onClick={() =>
                        void run("bank-match-groups", {
                          bankTransactionIds: s.bankTransactionIds,
                          journalLineIds: s.journalLineIds,
                          reason:
                            "Reviewed amount and accounting-date suggestion",
                        })
                      }
                    >
                      Prepare proposal
                    </Button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      </TableFrame>
      {!recordOnly && data.pages && (
        <div aria-label="Unmatched bank movement pages">
          <ListPagination
            query={query}
            cursorName="bankCursor"
            {...data.pages.bank}
            count={data.rows.length}
          />
        </div>
      )}
      {canWrite && !recordOnly && (
        <FormDisclosure error={error}
          appearance="standard"
          drawerTitle="Match multiple bank movements"
          onDiscard={() => {
            setSelectedBanks([]);
            setSelectedCash([]);
          }}
        >
          <summary>Choose movements to match</summary>
          <FormLayout data-finance-draft={draftScope} data-bank-match-selection
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void run("bank-match-groups", {
                bankTransactionIds: selectedBanks.map((row) => row.id),
                journalLineIds: selectedCash.map((row) => row.id),
                reason: f.get("reason"),
              });
            }}
          >
            <p>
              {selectedBanks.length} bank movement(s) and {selectedCash.length}{" "}
              cash line(s) selected across pages.
            </p>
            {(selectedBanks.length > 0 || selectedCash.length > 0) && (
              <div>
                <Button
                  appearance="standard"
                  onClick={() => {
                    setSelectedBanks([]);
                    setSelectedCash([]);
                  }}
                >
                  Clear selections
                </Button>
                <ul>
                  {selectedBanks.map((row) => (
                    <li key={String(row.id)}>
                      {String(row.transaction_date)} ·{" "}
                      {minorToDecimal(Number(row.amount_minor))}{" "}
                      {String(row.currency)}{" "}
                      <Button
                        appearance="standard"
                        onClick={() =>
                          setSelectedBanks((items) =>
                            items.filter((item) => item.id !== row.id),
                          )
                        }
                      >
                        Remove bank movement
                      </Button>
                    </li>
                  ))}
                  {selectedCash.map((row) => (
                    <li key={String(row.id)}>
                      {String(row.entry_date)} ·{" "}
                      {minorToDecimal(
                        Number(row.debit_minor) - Number(row.credit_minor),
                      )}{" "}
                      {String(row.currency)}{" "}
                      <Button
                        appearance="standard"
                        onClick={() =>
                          setSelectedCash((items) =>
                            items.filter((item) => item.id !== row.id),
                          )
                        }
                      >
                        Remove cash line
                      </Button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <fieldset>
              <legend>Unmatched bank movements</legend>
              {data.rows.map((r) => (
                <label key={String(r.id)}>
                  <input
                    type="checkbox"
                    name="banks"
                    value={String(r.id)}
                    checked={selectedBanks.some((row) => row.id === r.id)}
                    onChange={(event) =>
                      setSelectedBanks((items) =>
                        event.target.checked
                          ? [...items.filter((row) => row.id !== r.id), r]
                          : items.filter((row) => row.id !== r.id),
                      )
                    }
                  />{" "}
                  {String(r.transaction_date)} · {String(r.description)} ·{" "}
                  {minorToDecimal(Number(r.amount_minor))} {String(r.currency)}
                  <br />
                </label>
              ))}
              {data.pages && (
                <ListPagination
                  query={query}
                  cursorName="bankCursor"
                  {...data.pages.bank}
                  count={data.rows.length}
                />
              )}
            </fieldset>
            <fieldset>
              <legend>Posted cash lines</legend>
              <Field appearance="standard">
                Search posted cash lines
                <TextInput
                  appearance="standard"
                  name="cashQuery"
                  defaultValue={query.cashQuery ?? ""}
                />
              </Field>
              <Button
                appearance="standard"
                onClick={(event) => {
                  const form = event.currentTarget.closest("form");
                  const params = new URLSearchParams(
                    Object.entries(query).flatMap(([key, value]) =>
                      value && key !== "cashCursor" && key !== "cashQuery"
                        ? [[key, value]]
                        : [],
                    ),
                  );
                  const value = String(
                    new FormData(form ?? undefined).get("cashQuery") ?? "",
                  ).trim();
                  if (value) params.set("cashQuery", value);
                  router.push(canonicalUiHref(`${base}/finance/bank?${params}`));
                }}
              >
                Search cash lines
              </Button>
              {data.targets.map((r) => (
                <label key={String(r.id)}>
                  <input
                    type="checkbox"
                    name="lines"
                    value={String(r.id)}
                    checked={selectedCash.some((row) => row.id === r.id)}
                    onChange={(event) =>
                      setSelectedCash((items) =>
                        event.target.checked
                          ? [...items.filter((row) => row.id !== r.id), r]
                          : items.filter((row) => row.id !== r.id),
                      )
                    }
                  />{" "}
                  {String(r.entry_date)} ·{" "}
                  {minorToDecimal(
                    Number(r.debit_minor) - Number(r.credit_minor),
                  )}{" "}
                  {String(r.currency)} · {String(r.description ?? "")} ·{" "}
                  <RecordLink href={canonicalUiHref(`${base}/finance/journals/${r.journal_id}`)}>
                    Source journal
                  </RecordLink>
                  <br />
                </label>
              ))}
              {data.pages && (
                <ListPagination
                  query={query}
                  cursorName="cashCursor"
                  {...data.pages.cash}
                  count={data.targets.length}
                />
              )}
            </fieldset>
            <Field appearance="standard">
              Evidence and reason
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
              disabled={!canSettle || busy || !selectedBanks.length || !selectedCash.length}
            >
              Prepare match group
            </Button>
          </FormLayout>
        </FormDisclosure>
      )}
      <TableFrame appearance="standard" label="Bank match groups">
        <Table style={{minWidth:"42rem"}}>
          <thead>
            <tr>
              <th>Group / evidence</th>
              <th>Status</th>
              <th>Review</th>
            </tr>
          </thead>
          <tbody>
            {data.groups.map((g) => (
              <tr key={String(g.id)}>
                <td>
                  <details>
                    <summary>
                      <RecordLink
                        href={recordCanonicalHref(base, {
                          kind: "bank_match_group",
                          id: String(g.id),
                        })}
                      >
                        {String(g.currency)} · {String(g.id).slice(0, 8)}
                      </RecordLink>
                    </summary>
                    <p>
                      {String(
                        JSON.parse(String(g.explanation_json)).input?.reason ??
                          "Legacy source evidence requires review",
                      )}
                    </p>
                    <ul>
                      {(
                        (JSON.parse(String(g.explanation_json)).source?.lines ??
                          []) as Row[]
                      ).map((l) => (
                        <li key={String(l.id)}>
                          <RecordLink
                            href={canonicalUiHref(`${base}/finance/journals/${l.journal_id}`)}
                          >
                            {String(l.entry_date)} ·{" "}
                            {minorToDecimal(
                              Number(l.debit_minor) - Number(l.credit_minor),
                            )}
                          </RecordLink>
                        </li>
                      ))}
                    </ul>
                  </details>
                </td>
                <td>{String(g.status)}</td>
                <td>
                  <FormDisclosure appearance="standard" drawerTitle={`Bank match review · ${String(g.currency)}`} error={error} dismissDisabled={busy}>
                  <summary>Evidence and review</summary>
                  <p>Current status: {String(g.status)}</p>
                  <p>{String(JSON.parse(String(g.explanation_json)).input?.reason ?? "Legacy source evidence requires review")}</p>
                  <ul>{((JSON.parse(String(g.explanation_json)).source?.banks ?? []) as Row[]).map(r=><li key={String(r.id)}>{String(r.external_id)} · {String(r.transaction_date)} · {minorToDecimal(Number(r.amount_minor))} {String(r.currency)}</li>)}</ul>
                  <ul>{((JSON.parse(String(g.explanation_json)).source?.lines ?? []) as Row[]).map(l=><li key={String(l.id)}><RecordLink href={canonicalUiHref(`${base}/finance/journals/${l.journal_id}`)}>{String(l.entry_date)} · {minorToDecimal(Number(l.debit_minor)-Number(l.credit_minor))} {String(g.currency)}</RecordLink></li>)}</ul>
                  <FinanceEvidenceDocuments
                    base={base}
                    row={g}
                    targetType="finance_bank_match_group"
                    canWrite={canWrite}
                  />
                  {canApprove &&
                    (g.status === "matched" ||
                      (g.status === "proposed" &&
                        g.proposed_by !== userId)) && (
                      <FormLayout data-finance-draft={draftScope}
                        onSubmit={(e) => {
                          e.preventDefault();
                          const f = new FormData(e.currentTarget);
                          void run(
                            `bank-match-groups/${g.id}/${f.get("action")}`,
                            {
                              expectedVersion: Number(g.version),
                              reason: f.get("reason"),
                            },
                          );
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
                        <Field appearance="standard">Review action<Select appearance="standard" name="action">
                          {g.status === "matched" ? (
                            <option value="reverse">Reverse matching</option>
                          ) : (
                            <>
                              <option value="confirm">Confirm matching</option>
                              <option value="reject">Reject proposal</option>
                            </>
                          )}
                        </Select></Field>
                        <Button
                          appearance="standard"
                          clientManaged
                          type="submit"
                          disabled={!canSettle || busy}
                        >
                          Confirm review
                        </Button>
                      </FormLayout>
                    )}
                  </FormDisclosure>
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      </TableFrame>
      {!recordOnly && data.pages && (
        <div aria-label="Bank match group pages">
          <ListPagination
            query={query}
            cursorName="groupCursor"
            {...data.pages.groups}
            count={data.groups.length}
          />
        </div>
      )}
    </Card>
  );
}
