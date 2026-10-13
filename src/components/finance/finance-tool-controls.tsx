"use client";
import { useState } from "react";
import { useRouter } from "next/navigation";
import { financeCommandFetch as workFetch } from "@/lib/finance/finance-command-fetch";
import {
  Button,
  Card,
  Field,
  FormLayout,
  TextInput,
  InlineNotice,
} from "@/components/ui/foundation";
type Tool =
  | "planning"
  | "bank_automation"
  | "collections"
  | "business_contracts"
  | "instruments"
  | "asset_policies"
  | "group_reporting";
export function FinanceToolControls({
  base,
  rows,
  canWrite,
}: {
  base: string;
  rows: { tool: Tool; enabled: number; version: number }[];
  canWrite: boolean;
}) {
  const router = useRouter(),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    labels: { tool: Tool; label: string }[] = [
      {
        tool: "group_reporting",
        label: "Reviewed group reporting, ownership and acquisition evidence",
      },
      {
        tool: "instruments",
        label: "Reviewed leases, fixed-rate loans and repayments",
      },
      {
        tool: "asset_policies",
        label: "Asset depreciation policies for every book",
      },
      {
        tool: "business_contracts",
        label: "Business contracts, billing and revenue recognition",
      },
      { tool: "planning", label: "Budgets, forecasts and saved reports" },
      {
        tool: "bank_automation",
        label: "Saved bank CSV mappings and match groups",
      },
      {
        tool: "collections",
        label: "Collection cases and cash due-date forecasts",
      },
    ];
  return (
    <Card appearance="standard" as="section" className="ui-panel">
      <h2>Company Finance tools</h2>
      <p className="muted">
        Enable the tools this Company uses. Disabling a tool keeps its records
        available and stops new preparation.
      </p>
      {error && <InlineNotice tone="danger">{error}</InlineNotice>}
      {labels.map((t) => {
        const current = rows.find((r) => r.tool === t.tool);
        return (
          <FormLayout
            key={t.tool}
            className="ui-form-grid"
            onSubmit={async (e) => {
              e.preventDefault();
              setBusy(true);
              setError("");
              const f = new FormData(e.currentTarget);
              try {
                const r = await workFetch(
                    `${base.replace("/holdings/", "/api/v1/holdings/")}/finance/company-tools`,
                    {
                      method: "POST",
                      headers: {
                        "Content-Type": "application/json",
                        "Idempotency-Key": crypto.randomUUID(),
                      },
                      body: JSON.stringify({
                        tool: t.tool,
                        enabled: !current?.enabled,
                        expectedVersion: current?.version ?? 0,
                        reason: f.get("reason"),
                      }),
                    },
                  ),
                  body = await r.json();
                if (!r.ok)
                  throw new Error(body.error?.message ?? "Tool setting failed");
                router.refresh();
              } catch (e) {
                setError(
                  e instanceof Error ? e.message : "Tool setting failed",
                );
              } finally {
                setBusy(false);
              }
            }}
          >
            <b>
              {t.label} · {current?.enabled ? "Enabled" : "Off"}
            </b>
            {canWrite && (
              <>
                <Field appearance="standard">
                  Reason
                  <TextInput
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
                  {current?.enabled ? "Disable" : "Enable"}
                </Button>
              </>
            )}
          </FormLayout>
        );
      })}
    </Card>
  );
}
