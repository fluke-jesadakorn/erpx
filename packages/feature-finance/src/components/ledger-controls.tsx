"use client";
import { useFinanceDraftScope, useFinanceDraftClose, captureFinanceForm, acknowledgeFinanceForm } from "@erpx/feature-finance/components/finance-draft-boundary";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Dialog, FormDrawer } from "@erpx/ui/components/dialog";
import { Button, Field, Select, Table, TableFrame, TextInput } from "@erpx/ui/components/foundation";

import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import type { LedgerRow } from "@erpx/server-finance/public/ledger";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@erpx/feature-files/components/attachment-panel";
import { Card, Stack, FormGrid, FormLayout } from "@erpx/ui/components/foundation";
import { useFinanceActionAllowed } from "@erpx/feature-finance/components/purchase-context";
import {financeDocumentCommandAllowed, financeDocumentCommandContext, financeDocumentFormUnchanged, type FinanceDocumentPostControl} from "@erpx/feature-finance/lib/document-post-control";
import { NativeJournalPost } from "./native-journal-post";
import type { PostContext } from "../application/native-journal-post";
import {announceWorkChange} from "@erpx/feature-work_projection/lib/work-refresh";
function useFinanceCommand(base: string, purpose: "new-work" | "settle-existing" = "settle-existing", capability = "finance.accounting", post?: FinanceDocumentPostControl) {
  const allowed = useFinanceActionAllowed(purpose, capability);
  const router = useRouter();
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const attempt = useRef<{ input: string; key: string } | null>(null);
  const [hasAttempt, setHasAttempt] = useState(false);
  const [notice,setNotice] = useState("");
  const pendingRef = useRef(false);
  async function send(path: string, input: unknown) {
    if (pendingRef.current) return false;
    const body = JSON.stringify(input), identity = JSON.stringify([financeDocumentCommandContext(),path,body]);
    if (!financeDocumentCommandAllowed(allowed,path,post) && attempt.current?.input !== identity) {
      setError("This Finance action is unavailable. Your draft is retained.");
      return;
    }
    if (attempt.current?.input !== identity)
      { attempt.current = { input: identity, key: crypto.randomUUID() }; setHasAttempt(true); }
    const completion = captureFinanceForm();
    setPending(true);
    pendingRef.current = true; setNotice(""); setError("");
    try {
      const r = await workFetch(
        `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/${path}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body,
        }, completion, "caller",
      );
      const result = (await r.json()) as { data: { id?: string }; error?: { message: string } };
      if (!r.ok || !result.data || typeof result.data.id !== "string" || !result.data.id) throw new Error(result.error?.message ?? "Unable to save.");
      acknowledgeFinanceForm(completion); announceWorkChange();
      attempt.current = null;
      setHasAttempt(false);
      if (!financeDocumentFormUnchanged(completion)) {setNotice("Submitted change saved. Your newer draft is retained."); return false;}
      if (path === "journals") router.push(canonicalUiHref(`${base}/finance/journals/${result.data.id}`));
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save.");
    } finally {
      pendingRef.current = false; setPending(false);
    }
  }
  return { send, pending, error, notice, canSend: allowed || hasAttempt, canPostSend: (post ? financeDocumentCommandAllowed(allowed,post.path,post) : allowed) || hasAttempt };
}
export function JournalEditor({
  base,
  chart,
  accounts,
  periods,
  dimensions = [],
  dimensionValues = [],
  initialOpen = false,
  draft,
  draftLines=[],
}: {
  base: string;
  chart: LedgerRow;
  accounts: LedgerRow[];
  periods: LedgerRow[];
  dimensions?:LedgerRow[];
  dimensionValues?:LedgerRow[];
  initialOpen?: boolean;
  draft?:LedgerRow;
  draftLines?:LedgerRow[];
}) {
  const draftScope = useFinanceDraftScope();
  const { send, pending, error, canSend } = useFinanceCommand(base, "new-work");
  const [open, setOpen] = useState(initialOpen);
  const closeEditor = useFinanceDraftClose(() => setOpen(false));
  const [lines, setLines] = useState<Array<{accountId:string;debit:string;credit:string;description?:string;dimensions?:Record<string,string>}>>(draft?draftLines.map(l=>({accountId:String(l.account_id),debit:minorToDecimal(Number(l.debit_minor)),credit:minorToDecimal(Number(l.credit_minor)),description:String(l.description??""),dimensions:JSON.parse(String(l.dimension_json??"{}"))})):[
    { accountId: "", debit: "0", credit: "0" },
    { accountId: "", debit: "0", credit: "0" },
  ]);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <div className="ui-section-header">
        <h2>{draft?"Edit manual draft":"Manual journal"}</h2>
        {!initialOpen && (
          <Button appearance="standard" type="submit" variant="primary" disabled={!canSend} onClick={() => open ? closeEditor() : setOpen(true)}>
            {open ? "Close form" : draft?"Edit draft":"Create journal"}
          </Button>
        )}
      </div>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {open && (
        <FormDrawer appearance="standard" inline={initialOpen} open={open} onClose={closeEditor} title={draft?"Edit manual draft":"Create journal"} dismissDisabled={pending}><Stack as="div" className="ui-stack">{error && <InlineNotice tone="danger" role="alert" className="error">{error}</InlineNotice>}<form data-finance-draft={draftScope}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(draft?`journals/${draft.id}/edit`:"journals", {
              ...(draft?{expectedVersion:Number(draft.version),reason:f.get("reason")}:{sourceType:"manual"}),
              chartId: chart.id,
              periodId: f.get("periodId"),
              entryDate: f.get("entryDate"),
              currency: draft?.currency??chart.base_currency,
              memo: f.get("memo"),
              lines,
            });
          }}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Accounting period
              <Select appearance="standard" name="periodId" required defaultValue={draft?String(draft.period_id):undefined}>
                <option value="">Select period</option>
                {periods
                  .filter((p) => p.state === "open")
                  .map((p) => (
                    <option key={String(p.id)} value={String(p.id)}>
                      {String(p.name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Posting date
              <TextInput appearance="standard"
                name="entryDate"
                type="date"
                required
                defaultValue={draft?String(draft.entry_date):new Date().toISOString().slice(0, 10)}
              />
            </Field>
            <Field appearance="standard">
              Journal memo
              <TextInput appearance="standard" name="memo" required maxLength={2000} defaultValue={draft?String(draft.memo):undefined} />
            </Field>
            {draft&&<Field appearance="standard">Reason for this draft revision<TextInput appearance="standard" name="reason" required minLength={10} maxLength={2000}/></Field>}
          </FormGrid>
          {lines.map((l, i) => (
            <FormGrid as="div" className="ui-form-grid" key={i}>
              <Field appearance="standard">
                Account {i + 1}
                <Select appearance="standard"
                  required
                  value={l.accountId}
                  onChange={(e) =>
                    setLines(
                      lines.map((v, j) => (j === i ? { ...v, accountId: e.target.value } : v)),
                    )
                  }
                >
                  <option value="">Select account</option>
                  {accounts
                    .filter((a) => a.status === "active")
                    .map((a) => (
                      <option key={String(a.id)} value={String(a.id)}>
                        {String(a.code)} · {String(a.name)}
                      </option>
                    ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Debit {i + 1}
                <TextInput appearance="standard"
                  type="number"
                  min="0"
                  step="0.01"
                  required
                  value={l.debit}
                  onChange={(e) =>
                    setLines(lines.map((v, j) => (j === i ? { ...v, debit: e.target.value } : v)))
                  }
                />
              </Field>
              <Field appearance="standard">
                Credit {i + 1}
                <TextInput appearance="standard"
                  type="number"
                  min="0"
                  step="0.01"
                  required
                  value={l.credit}
                  onChange={(e) =>
                    setLines(lines.map((v, j) => (j === i ? { ...v, credit: e.target.value } : v)))
                  }
                />
              </Field>
              {dimensions.map(d=><Field key={String(d.id)} appearance="standard">{String(d.name)} · line {i+1}<Select appearance="standard" required={Number(d.required)===1} value={l.dimensions?.[String(d.code)]??''} onChange={e=>setLines(lines.map((v,j)=>j===i?{...v,dimensions:Object.fromEntries(Object.entries({...v.dimensions,[String(d.code)]:e.target.value}).filter(([,value])=>value))}:v))}><option value="">Choose {String(d.name)}</option>{dimensionValues.filter(v=>v.dimension_id===d.id).map(v=><option key={String(v.id)} value={String(v.code)}>{String(v.code)} · {String(v.name)}</option>)}</Select></Field>)}
              {lines.length > 2 && (
                <Button appearance="standard"
                  variant="secondary"
                  type="button"
                  onClick={() => setLines(lines.filter((_, j) => j !== i))}
                >
                  Remove line
                </Button>
              )}
            </FormGrid>
          ))}
          <div className="ui-form-actions">
            <Button appearance="standard"
              variant="secondary"
              type="button"
              onClick={() => setLines([...lines, { accountId: "", debit: "0", credit: "0" }])}
            >
              Add line
            </Button>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending || !canSend}>
              Save draft
            </Button>
            <span className="muted">
              Currency {String(chart.base_currency)} · another finance approver must post this
              journal
            </span>
          </div>
        </form></Stack></FormDrawer>
      )}
    </Card>
  );
}

type OpeningLine = {
  accountId: string;
  debit: string;
  credit: string;
  description?: string;
  subledgerKind: "none" | "receivable" | "payable";
};
type OpeningItem = {
  kind: "receivable" | "payable";
  partyId: string;
  reference: string;
  dueDate: string;
  controlAccountId: string;
  offsetAccountId: string;
  amount: string;
};
type OpeningDefinition = {
  chartId: string;
  periodId: string;
  entryDate: string;
  currency: string;
  lines: OpeningLine[];
  subledgerItems: OpeningItem[];
};
type OpeningPreview = {
  previewToken: string;
  currency: string;
  debit: string;
  credit: string;
  subledgerCount: number;
  lines: Array<OpeningLine & { accountCode: string; accountName: string }>;
  subledgerItems: OpeningItem[];
  definition: OpeningDefinition;
};
const blankOpeningLine = (): OpeningLine => ({
  accountId: "",
  debit: "0",
  credit: "0",
  subledgerKind: "none",
});
const blankOpeningItem = (): OpeningItem => ({
  kind: "receivable",
  partyId: "",
  reference: "",
  dueDate: "",
  controlAccountId: "",
  offsetAccountId: "",
  amount: "0",
});
export function OpeningBalanceImport({
  base,
  chart,
  accounts,
  periods,
  parties,
}: {
  base: string;
  chart: LedgerRow;
  accounts: LedgerRow[];
  periods: LedgerRow[];
  parties: LedgerRow[];
}) {
  const draftScope = useFinanceDraftScope();
  const router = useRouter(),
    form = useRef<HTMLFormElement>(null),
    attempt = useRef<{ body: string; key: string } | null>(null),
    [open, setOpen] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [preview, setPreview] = useState<OpeningPreview | null>(null),
    [lines, setLines] = useState<OpeningLine[]>([blankOpeningLine(), blankOpeningLine()]),
    [items, setItems] = useState<OpeningItem[]>([]);
  const endpoint = `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/opening-balances`;
  const invalidateLines = (next: OpeningLine[]) => {
    setLines(next);
    setPreview(null);
  };
  const invalidateItems = (next: OpeningItem[]) => {
    setItems(next);
    setPreview(null);
  };
  const definition = (): OpeningDefinition => {
    const values = new FormData(form.current!);
    return {
      chartId: String(chart.id),
      periodId: String(values.get("periodId")),
      entryDate: String(values.get("entryDate")),
      currency: String(chart.base_currency),
      lines,
      subledgerItems: items,
    };
  };
  const closeEditor = useFinanceDraftClose(() => setOpen(false));
  async function requestPreview() {
    setBusy(true);
    setError("");
    try {
      const input = definition(),
        response = await workFetch(`${endpoint}/preview`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(input),
        }),
        body = (await response.json()) as {
          data: Omit<OpeningPreview, "definition">;
          error?: { message: string };
        };
      if (!response.ok)
        throw new Error(body.error?.message ?? "Unable to preview opening balances.");
      setPreview({ ...body.data, definition: input });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to preview opening balances.");
    } finally {
      setBusy(false);
    }
  }
  async function commit() {
    if (!preview) return;
    const completion = captureFinanceForm();
    setBusy(true);
    setError("");
    const body = JSON.stringify({ ...preview.definition, previewToken: preview.previewToken });
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`${endpoint}/import`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body,
        }),
        result = (await response.json()) as { data: { id: string }; error?: { message: string } };
      if (!response.ok || !result.data || typeof result.data.id !== "string" || !result.data.id)
        throw new Error(result.error?.message ?? "Unable to import opening balances.");
      acknowledgeFinanceForm(completion);
      attempt.current = null;
      router.push(canonicalUiHref(`${base}/finance/journals/${result.data.id}`));
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to import opening balances.");
    } finally {
      setBusy(false);
    }
  }
  const controlLines = lines.filter((line) => line.subledgerKind !== "none" && line.accountId);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <div className="ui-section-header">
        <div>
          <h2>Opening balances</h2>
          <p className="muted">
            Reconcile the trial balance to customer and supplier schedules before independent
            approval
          </p>
        </div>
        <Button appearance="standard" variant="secondary" type="button" onClick={() => open ? closeEditor() : setOpen(true)}>
          {open ? "Close form" : "Import opening balances"}
        </Button>
      </div>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {open && (
        <Dialog appearance="standard" open={open} onClose={closeEditor} title="Import opening balances" presentation="form-drawer" dismissDisabled={busy}><Stack as="div" className="ui-stack">{error && <InlineNotice tone="danger" role="alert" className="error">{error}</InlineNotice>}<form data-finance-draft={draftScope}
          ref={form}
          onSubmit={(e) => {
            e.preventDefault();
            void requestPreview();
          }}
          onChange={() => setPreview(null)}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Opening period
              <Select appearance="standard" name="periodId" required defaultValue="">
                <option value="">Select an open period</option>
                {periods
                  .filter((period) => period.state === "open")
                  .map((period) => (
                    <option key={String(period.id)} value={String(period.id)}>
                      {String(period.name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Opening balance date
              <TextInput appearance="standard" name="entryDate" type="date" required />
            </Field>
            <Field appearance="standard">
              Currency
              <TextInput appearance="standard" value={String(chart.base_currency)} readOnly />
            </Field>
          </FormGrid>
          <h3>General ledger balances</h3>
          {lines.map((line, index) => {
            const selected = accounts.find((account) => account.id === line.accountId),
              isControl = Number(selected?.is_control) === 1,
              configured = String(selected?.control_kind ?? "none");
            return (
              <FormGrid as="div" className="ui-form-grid" key={index}>
                <Field appearance="standard">
                  Account {index + 1}
                  <Select appearance="standard"
                    value={line.accountId}
                    required
                    onChange={(e) => {
                      const account = accounts.find((value) => value.id === e.target.value),
                        kind = String(account?.control_kind ?? "none");
                      invalidateLines(
                        lines.map((item, i) =>
                          i === index
                            ? {
                                ...item,
                                accountId: e.target.value,
                                subledgerKind:
                                  kind === "receivable" || kind === "payable" ? kind : "none",
                              }
                            : item,
                        ),
                      );
                    }}
                  >
                    <option value="">Select account</option>
                    {accounts
                      .filter((account) => account.status === "active")
                      .map((account) => (
                        <option value={String(account.id)} key={String(account.id)}>
                          {String(account.code)} · {String(account.name)}
                          {Number(account.is_control) === 1
                            ? ` · ${String(account.control_kind) === "none" ? "control" : account.control_kind}`
                            : ""}
                        </option>
                      ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Debit {index + 1}
                  <TextInput appearance="standard"
                    type="number"
                    min="0"
                    step="0.01"
                    value={line.debit}
                    required
                    onChange={(e) =>
                      invalidateLines(
                        lines.map((item, i) =>
                          i === index ? { ...item, debit: e.target.value } : item,
                        ),
                      )
                    }
                  />
                </Field>
                <Field appearance="standard">
                  Credit {index + 1}
                  <TextInput appearance="standard"
                    type="number"
                    min="0"
                    step="0.01"
                    value={line.credit}
                    required
                    onChange={(e) =>
                      invalidateLines(
                        lines.map((item, i) =>
                          i === index ? { ...item, credit: e.target.value } : item,
                        ),
                      )
                    }
                  />
                </Field>
                <Field appearance="standard">
                  Subledger
                  <Select appearance="standard"
                    value={line.subledgerKind}
                    disabled={!isControl || configured !== "none"}
                    required={configured !== "none"}
                    onChange={(e) =>
                      invalidateLines(
                        lines.map((item, i) =>
                          i === index
                            ? {
                                ...item,
                                subledgerKind: e.target.value as OpeningLine["subledgerKind"],
                              }
                            : item,
                        ),
                      )
                    }
                  >
                    <option value="none">
                      {isControl ? "No customer/supplier schedule" : "Not a control account"}
                    </option>
                    <option value="receivable">Customer receivables</option>
                    <option value="payable">Supplier payables</option>
                  </Select>
                </Field>
                <Field appearance="standard">
                  Description
                  <TextInput appearance="standard"
                    value={line.description ?? ""}
                    maxLength={500}
                    onChange={(e) =>
                      invalidateLines(
                        lines.map((item, i) =>
                          i === index ? { ...item, description: e.target.value } : item,
                        ),
                      )
                    }
                  />
                </Field>
                {lines.length > 2 && (
                  <Button appearance="standard"
                    variant="secondary"
                    type="button"
                    onClick={() => invalidateLines(lines.filter((_, i) => i !== index))}
                  >
                    Remove line
                  </Button>
                )}
              </FormGrid>
            );
          })}
          <div className="ui-form-actions">
            <Button appearance="standard"
              variant="secondary"
              type="button"
              onClick={() => invalidateLines([...lines, blankOpeningLine()])}
            >
              Add GL line
            </Button>
          </div>
          <div className="ui-section-header">
            <div>
              <h3>Customer and supplier schedule</h3>
              <p className="muted">
                Each schedule must total exactly to its selected control-account balance.
              </p>
            </div>
            <Button appearance="standard"
              variant="secondary"
              type="button"
              disabled={!controlLines.length}
              onClick={() => invalidateItems([...items, blankOpeningItem()])}
            >
              Add subledger item
            </Button>
          </div>
          {items.map((item, index) => (
            <FormGrid as="div" className="ui-form-grid" key={index}>
              <Field appearance="standard">
                Type
                <Select appearance="standard"
                  value={item.kind}
                  onChange={(e) =>
                    invalidateItems(
                      items.map((value, i) =>
                        i === index
                          ? {
                              ...value,
                              kind: e.target.value as OpeningItem["kind"],
                              partyId: "",
                              controlAccountId: "",
                            }
                          : value,
                      ),
                    )
                  }
                >
                  <option value="receivable">Receivable</option>
                  <option value="payable">Payable</option>
                </Select>
              </Field>
              <Field appearance="standard">
                {item.kind === "receivable" ? "Customer" : "Supplier"}
                <Select appearance="standard"
                  required
                  value={item.partyId}
                  onChange={(e) =>
                    invalidateItems(
                      items.map((value, i) =>
                        i === index ? { ...value, partyId: e.target.value } : value,
                      ),
                    )
                  }
                >
                  <option value="">Select party</option>
                  {parties
                    .filter(
                      (p) =>
                        p.kind === (item.kind === "receivable" ? "customer" : "supplier") &&
                        p.currency === chart.base_currency,
                    )
                    .map((p) => (
                      <option key={String(p.id)} value={String(p.id)}>
                        {String(p.code)} · {String(p.name)}
                      </option>
                    ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Reference
                <TextInput appearance="standard"
                  required
                  maxLength={120}
                  value={item.reference}
                  onChange={(e) =>
                    invalidateItems(
                      items.map((value, i) =>
                        i === index ? { ...value, reference: e.target.value } : value,
                      ),
                    )
                  }
                />
              </Field>
              <Field appearance="standard">
                Due date
                <TextInput appearance="standard"
                  required
                  type="date"
                  value={item.dueDate}
                  onChange={(e) =>
                    invalidateItems(
                      items.map((value, i) =>
                        i === index ? { ...value, dueDate: e.target.value } : value,
                      ),
                    )
                  }
                />
              </Field>
              <Field appearance="standard">
                Control account
                <Select appearance="standard"
                  required
                  value={item.controlAccountId}
                  onChange={(e) =>
                    invalidateItems(
                      items.map((value, i) =>
                        i === index ? { ...value, controlAccountId: e.target.value } : value,
                      ),
                    )
                  }
                >
                  <option value="">Select control account</option>
                  {controlLines
                    .filter((line) => line.subledgerKind === item.kind)
                    .map((line) => {
                      const account = accounts.find((a) => a.id === line.accountId);
                      return (
                        <option key={line.accountId} value={line.accountId}>
                          {String(account?.code)} · {String(account?.name)}
                        </option>
                      );
                    })}
                </Select>
              </Field>
              <Field appearance="standard">
                Offset account
                <Select appearance="standard"
                  required
                  value={item.offsetAccountId}
                  onChange={(e) =>
                    invalidateItems(
                      items.map((value, i) =>
                        i === index ? { ...value, offsetAccountId: e.target.value } : value,
                      ),
                    )
                  }
                >
                  <option value="">Select offset account</option>
                  {accounts
                    .filter(
                      (account) =>
                        account.status === "active" && account.id !== item.controlAccountId,
                    )
                    .map((account) => (
                      <option key={String(account.id)} value={String(account.id)}>
                        {String(account.code)} · {String(account.name)}
                      </option>
                    ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Amount
                <TextInput appearance="standard"
                  required
                  type="number"
                  min="0.01"
                  step="0.01"
                  value={item.amount}
                  onChange={(e) =>
                    invalidateItems(
                      items.map((value, i) =>
                        i === index ? { ...value, amount: e.target.value } : value,
                      ),
                    )
                  }
                />
              </Field>
              <Button appearance="standard"
                variant="secondary"
                type="button"
                onClick={() => invalidateItems(items.filter((_, i) => i !== index))}
              >
                Remove item
              </Button>
            </FormGrid>
          ))}
          <div className="ui-form-actions">
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              {busy ? "Checking…" : "Preview reconciliation"}
            </Button>
          </div>
          {preview && (
            <Card as="section" appearance="standard" className="ui-panel" aria-live="polite">
              <h3>Opening reconciliation</h3>
              <p>
                <strong>
                  {preview.currency} {preview.debit}
                </strong>{" "}
                Debit ·{" "}
                <strong>
                  {preview.currency} {preview.credit}
                </strong>{" "}
                Credit · {preview.lines.length} accounts · <strong>{preview.subledgerCount}</strong>{" "}
                subledger item(s)
              </p>
              <TableFrame appearance="standard" label="Records" className="business-table-wrap">
                <Table className="business-table">
                  <thead>
                    <tr>
                      <th>Account</th>
                      <th>Schedule</th>
                      <th>Debit</th>
                      <th>Credit</th>
                    </tr>
                  </thead>
                  <tbody>
                    {preview.lines.map((line) => (
                      <tr key={line.accountId}>
                        <td>
                          {line.accountCode} · {line.accountName}
                        </td>
                        <td>{line.subledgerKind === "none" ? "—" : line.subledgerKind}</td>
                        <td>{line.debit}</td>
                        <td>{line.credit}</td>
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </TableFrame>
              <Button appearance="standard" variant="primary" type="button" disabled={busy} onClick={() => void commit()}>
                {busy ? "Importing…" : "Create reconciled opening journal"}
              </Button>
            </Card>
          )}
        </form></Stack></Dialog>
      )}
    </Card>
  );
}
export function JournalAction({
  base,
  journal,
  canPost,
  sourceCanPost,
  periods,
  nativePost,
}: {
  base: string;
  journal: LedgerRow;
  nativePost?: { context?: PostContext };
  canPost: boolean;
  sourceCanPost?: boolean;
  periods: LedgerRow[];
}) {
  const draftScope = useFinanceDraftScope();
  const { send, pending, error, notice, canSend, canPostSend } = useFinanceCommand(base,"settle-existing","finance.accounting",{path:`journals/${journal.id}/post`,canPost:sourceCanPost});
  return (
    <section>
      {notice && <p role="status">{notice}</p>}{error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {nativePost ? nativePost.context ? <NativeJournalPost url={`${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/journals/${encodeURIComponent(String(journal.id))}`} id={String(journal.id)} version={journal.version} context={nativePost.context} allowed={canPost && canPostSend && ["draft", "in_review"].includes(String(journal.status))} /> : <InlineNotice tone="danger" role="alert">The current work context cannot be confirmed.</InlineNotice> : canPost && canPostSend && ["draft", "in_review"].includes(String(journal.status)) ? <Button appearance="standard"
          type="submit"
          variant="primary"
          disabled={pending || !canPostSend}
          onClick={() =>
            void send(`journals/${journal.id}/post`, { expectedVersion: journal.version })
          }
        >
          Approve and post journal
        </Button> : null}
      {canPost &&
        journal.status === "posted" &&
        ["manual", "opening_import", "reversal"].includes(String(journal.source_type)) && (
          <FormLayout data-finance-draft={draftScope}
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send(`journals/${journal.id}/reverse`, {
                periodId: f.get("periodId"),
                entryDate: f.get("entryDate"),
                memo: f.get("memo"),
              });
            }}
          >
            <Field appearance="standard">
              Reversal period
              <Select appearance="standard" name="periodId" required>
                {periods
                  .filter((p) => p.state === "open")
                  .map((p) => (
                    <option key={String(p.id)} value={String(p.id)}>
                      {String(p.name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Reversal date
              <TextInput appearance="standard"
                name="entryDate"
                type="date"
                required
                defaultValue={new Date().toISOString().slice(0, 10)}
              />
            </Field>
            <Field appearance="standard">
              Reason
              <TextInput appearance="standard" name="memo" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={pending || !canSend}>
              Post reversing journal
            </Button>
          </FormLayout>
        )}
    </section>
  );
}
export function PeriodControls({
  base,
  period,
  canApprove,
  userId,
}: {
  base: string;
  period: {
    id: string | number;
    state: string | number | null;
    close_version?: string | number | null;
    close_started_by?: string | number | null;
    blockers?: string | number | null;
  };
  canApprove: boolean;
  userId?: string;
}) {
  const draftScope = useFinanceDraftScope();
  const { send, pending, error, canSend } = useFinanceCommand(base,"settle-existing","finance.close"),
    blocked = Number(period.blockers ?? 0) > 0,
    [files, setFiles] = useState<AttachmentPanelChange | null>(null);
  return (
    <div>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {period.state === "open" ? (
        <Button appearance="standard"
          type="submit"
          variant="secondary"
          disabled={pending || !canSend || blocked}
          onClick={() => void send(`periods/${period.id}/begin-close`, {})}
        >
          {blocked ? "Resolve blockers before close" : "Check readiness and begin close"}
        </Button>
      ) : (
        canApprove && (
          <FormLayout data-finance-draft={draftScope}
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              void send(`periods/${period.id}/${period.state === "closing" ? "close" : "reopen"}`, {
                expectedVersion: period.close_version,
                proof: new FormData(e.currentTarget).get("proof"),
                attachmentSetIds:
                  period.state === "closing" &&
                  files?.setId &&
                  files.readyCount > 0 &&
                  files.bindable
                    ? [files.setId]
                    : [],
              });
            }}
          >
            <Field appearance="standard">
              Review evidence
              <TextInput appearance="standard" name="proof" required minLength={3} />
            </Field>
            {period.state === "closing" && (
              <AttachmentPanel appearance="standard"
                base={base}
                targetType="finance_period"
                targetId={String(period.id)}
                expectedTargetVersion={Number(period.close_version)}
                componentKind="action"
                componentRef="close-approval"
                purposeKey="close_workpaper"
                title="Close workpapers"
                description="Attach the reviewed reconciliation and close support files for this period revision."
                visibility="Visible only to authorized Company Finance reviewers."
                onChange={setFiles}
              />
            )}
            <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={pending || !canSend}>
              {period.state === "closing" ? "Approve period close" : "Reopen period"}
            </Button>
          </FormLayout>
        )
      )}
      {period.state === "closing" && (canApprove || period.close_started_by === userId) && (
        <FormLayout data-finance-draft={draftScope}
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            void send(`periods/${period.id}/cancel-close`, {
              expectedVersion: Number(period.close_version),
              reason: new FormData(e.currentTarget).get("reason"),
            });
          }}
        >
          <Field appearance="standard">
            Reason to cancel close preparation
            <TextInput appearance="standard" name="reason" required minLength={3} maxLength={2000} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={pending || !canSend}>
            Cancel close preparation
          </Button>
          <p className="muted">
            Reopen preparation for corrections, review and begin close again. Prior evidence remains
            retained.
          </p>
        </FormLayout>
      )}
    </div>
  );
}
