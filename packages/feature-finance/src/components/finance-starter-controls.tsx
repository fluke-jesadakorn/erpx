"use client";
import { useFinanceActionAllowed } from "@erpx/feature-finance/components/purchase-context";
import { useFinanceDraftScope, captureFinanceForm, acknowledgeFinanceForm } from "@erpx/feature-finance/components/finance-draft-boundary";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { thaiFinanceStarterAccounts } from "@erpx/feature-finance/lib/finance-starter";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import {
  Button,
  Card,
  Field,
  FormLayout,
  InlineNotice,
  TextInput,
  TextArea,
  Table,
  TableFrame,
} from "@erpx/ui/components/foundation";

export function FinanceStarterControls({ base }: { base: string }) {
  const allowed = useFinanceActionAllowed("new-work"), draftScope = useFinanceDraftScope(), [hasAttempt,setHasAttempt] = useState(false);
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    retry = useRef<{ body: string; key: string } | null>(null);
  async function save(form: FormData) {
    const input = {
        preset: "thai_services_v1",
        expectedVersion: 0,
        name: String(form.get("name")),
        firstMonth: String(form.get("firstMonth")),
        organizationDimensions: form.get("organizationDimensions") === "on",
        reason: String(form.get("reason")),
      },
      body = JSON.stringify(input);
    if (!allowed && retry.current?.body !== body) {setError("This Finance action is unavailable. Your draft is retained.");return;}
    if (retry.current?.body !== body)
      retry.current = { body, key: crypto.randomUUID() };
    setHasAttempt(true);const completion=captureFinanceForm();
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(
          `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/setup/starter`,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Idempotency-Key": retry.current!.key,
            },
            body,
          },
        ),
        result = await response.json();
      if (!response.ok || !result.data || typeof result.data.id !== "string" || !result.data.id)
        throw new Error(
          result.error?.message ??
            "Unable to create the starter configuration.",
        );
      acknowledgeFinanceForm(completion);retry.current = null;setHasAttempt(false);
      router.push(
        canonicalUiHref(`${base}/finance/setup?step=profile&chart=${encodeURIComponent(result.data.id)}`),
      );
      router.refresh();
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : "Unable to create the starter configuration.",
      );
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card appearance="standard" as="section" className="ui-panel">
      <h2>Start with a Thai service ledger</h2>
      <p>
        THB accounts for customer billing, bank receipts, VAT, assets, leases
        and loans. Then review your Company accounting policy and opening
        balances.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert">
          {error}
        </InlineNotice>
      )}
      <FormDisclosure error={error}
        appearance="standard"
        drawerTitle="Create a starter ledger"
      >
        <summary>Use starter configuration</summary>
        <FormLayout data-finance-draft={draftScope}
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            void save(new FormData(e.currentTarget));
          }}
        >
          <Field appearance="standard">
            Ledger name
            <TextInput
              appearance="standard"
              name="name"
              defaultValue="Company THB ledger"
              required
              maxLength={160}
            />
          </Field>
          <Field appearance="standard">
            First accounting month
            <TextInput
              appearance="standard"
              type="month"
              name="firstMonth"
              defaultValue={new Date().toISOString().slice(0, 7)}
              required
            />
          </Field>
          <label>
            <input
              type="checkbox"
              name="organizationDimensions"
              defaultChecked
            />{" "}
            Include optional Branch and Department report dimensions from the
            current organization
          </label>
          <details>
            <summary>Review the starting accounts</summary>
            <TableFrame appearance="standard" label="Starter accounts">
              <Table>
                <thead>
                  <tr>
                    <th>Code</th>
                    <th>Account</th>
                    <th>Category</th>
                    <th>Control</th>
                  </tr>
                </thead>
                <tbody>
                  {thaiFinanceStarterAccounts.map((a) => (
                    <tr key={a.code}>
                      <td>{a.code}</td>
                      <td>{a.name}</td>
                      <td>{a.category.replaceAll("_", " ")}</td>
                      <td>{a.controlKind}</td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </TableFrame>
          </details>
          <p className="muted">
            The starting accounts include straight-line depreciation and
            deferred revenue accounts. Choose the actual recognition method and
            Company policy in their review forms. Add project and custom
            dimensions in Books &amp; dimensions.
          </p>
          <Field appearance="standard">
            Setup reason
            <TextArea
              appearance="standard"
              name="reason"
              required
              minLength={3}
              maxLength={2000}
            />
          </Field>
          <Button
            appearance="standard"
            clientManaged
            type="submit"
            disabled={busy || (!allowed && !hasAttempt)}
          >
            Create configuration and review policy
          </Button>
        </FormLayout>
      </FormDisclosure>
    </Card>
  );
}
