"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextInput } from "@erpx/ui/components/foundation";

import { useState } from "react";
import { isPilotModule } from "@erpx/feature-shared/lib/hospitality-rollout";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";

type Entry = {
  id: string;
  name: string;
  state: string;
  rollout: string;
  version: number;
  setupReady: boolean;
  setupChecked: boolean;
  productionAccepted: boolean;
  businessAccepted: boolean;
  permitted: boolean;
  delivery: string;
  environment: string;
  reason?: string;
};

export function PilotFeatureRollout({ base, initial }: { base: string; initial: Entry[] }) {
  const [entries, setEntries] = useState(initial);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [reasons, setReasons] = useState<Record<string, string>>({});
  const pilot = entries.filter((entry) => isPilotModule(entry.id));
  async function change(entry: Entry, state: "off" | "pilot") {
    const reason = reasons[entry.id]?.trim() ?? "";
    if (reason.length < 3) {
      setError("Enter a reason with at least three characters.");
      return;
    }
    setBusy(entry.id);
    setError("");
    setNotice("");
    try {
      const response = await workFetch(`/api/v1${base}/feature-catalogue`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ moduleId: entry.id, state, expectedVersion: entry.version, reason }),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Rollout change failed");
      const refreshed = await workFetch(`/api/v1${base}/feature-catalogue`, { cache: "no-store" });
      const updated = await refreshed.json();
      if (!refreshed.ok)
        throw new Error(
          updated.error?.message ?? "Rollout changed but the catalogue could not be reloaded",
        );
      setEntries(updated.data);
      setReasons((previous) => ({ ...previous, [entry.id]: "" }));
      setNotice(
        `${entry.name} is now ${state === "pilot" ? "in the pilot" : "off"}. Reload the workspace to update navigation.`,
      );
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Rollout change failed");
    } finally {
      setBusy("");
    }
  }
  return (
    <div style={{ maxWidth: 1000, margin: "0 auto", padding: "24px 20px 80px" }}>
      <p style={{ color: "var(--text-muted, #667085)" }}>ERPX · Company administration</p>
      <h1>Pilot feature rollout</h1>
      <p>
        Production starts with every module off. Activate only the hotel and restaurant modules
        approved for this company. The basic setup checks do not replace the accountant’s
        configuration review or business acceptance. Switching off a module stops new work;
        historical records stay readable where authorized. In development, preview workflows remain
        visible even when production rollout is off.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert" style={{ color: "#b42318" }}>
          {error}
        </InlineNotice>
      )}
      {notice && (
        <InlineNotice tone="info" role="status" style={{ color: "#067647" }}>
          {notice}
        </InlineNotice>
      )}
      <div style={{ display: "grid", gap: 16, marginTop: 24 }}>
        {pilot.map((entry) => (
          <article
            key={entry.id}
            style={{ border: "1px solid #d0d5dd", borderRadius: 12, padding: 20 }}
          >
            <div
              style={{
                display: "flex",
                gap: 16,
                justifyContent: "space-between",
                alignItems: "start",
                flexWrap: "wrap",
              }}
            >
              <div>
                <h2 style={{ marginTop: 0 }}>{entry.name}</h2>
                <p style={{ marginBottom: 4 }}>
                  {entry.environment === "development" ? "Development state" : "Production state"}:{" "}
                  <strong>{entry.state}</strong> · Production rollout: {entry.rollout} · Version{" "}
                  {entry.version}
                </p>
                <p style={{ margin: 0 }}>
                  Setup:{" "}
                  {entry.setupChecked ? (entry.setupReady ? "ready" : "required") : "not verified"}{" "}
                  · Your access: {entry.permitted ? "permitted" : "not granted"} · Formal evidence:{" "}
                  {entry.productionAccepted ? "accepted" : "pending"} · Business acceptance:{" "}
                  {entry.businessAccepted ? "accepted" : "pending"}
                </p>
                {entry.reason && <p>Last reason: {entry.reason}</p>}
              </div>
            </div>
            <Field appearance="standard" style={{ display: "grid", gap: 6, marginTop: 16, maxWidth: 560 }}>
              Reason for the change
              <TextInput appearance="standard"
                value={reasons[entry.id] ?? ""}
                onChange={(event) =>
                  setReasons((previous) => ({ ...previous, [entry.id]: event.target.value }))
                }
                minLength={3}
                maxLength={1000}
                style={{ padding: 10, border: "1px solid #98a2b3", borderRadius: 8 }}
              />
            </Field>
            <div style={{ display: "flex", gap: 8, marginTop: 12 }}>
              <Button appearance="standard"
                variant="ghost"
                type="button"
                disabled={Boolean(busy) || entry.rollout === "pilot"}
                onClick={() => void change(entry, "pilot")}
              >
                Activate pilot
              </Button>
              <Button appearance="standard"
                variant="ghost"
                type="button"
                disabled={Boolean(busy) || entry.rollout === "off"}
                onClick={() => void change(entry, "off")}
              >
                Switch off
              </Button>
            </div>
          </article>
        ))}
      </div>
    </div>
  );
}
