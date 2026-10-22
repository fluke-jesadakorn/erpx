"use client";
import { Button, Field, TextInput } from "@erpx/ui/components/foundation";

import { useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { type UiAppearance } from "@erpx/ui/components/foundation";

type Level = { id: string; name: string; level_number: number };
type Default = { level_id: string; version: number; annual_days: number; sick_days: number };
export function LevelLeaveDefaults({ appearance = "legacy",
  base,
  levels,
  defaults,
}: ({
  base: string;
  levels: Level[];
  defaults: Default[];
}) & { appearance?: UiAppearance }) {
  const router = useRouter(),
    [busy, setBusy] = useState<string | null>(null),
    [error, setError] = useState("");
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <h2 data-ui-appearance={appearance}>Leave defaults by level</h2>
      <p data-ui-appearance={appearance} className="muted">
        Annual and sick days are tracked. Saving creates a new policy version; existing assignments
        keep their policy history. A level change during the year takes effect for leave on the next
        1 January.
      </p>
      <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-stack" : "content-stack"}>
        {levels.map((level) => {
          const current = defaults.find((item) => item.level_id === level.id);
          return (
            <form data-ui-appearance={appearance}
              key={`${level.id}-${current?.version ?? 0}`}
              className={appearance === "standard" ? "ui-form-grid" : "business-form"}
              onSubmit={async (event) => {
                event.preventDefault();
                const fields = new FormData(event.currentTarget);
                setBusy(level.id);
                setError("");
                try {
                  const response = await workFetch(
                    `/api/v1${base}/people/organization/levels/${level.id}/default-leave`,
                    {
                      method: "POST",
                      headers: {
                        "Content-Type": "application/json",
                        "Idempotency-Key": crypto.randomUUID(),
                      },
                      body: JSON.stringify({
                        annualDays: Number(fields.get("annual")),
                        sickDays: Number(fields.get("sick")),
                        expectedVersion: current?.version ?? 0,
                      }),
                    },
                  );
                  const result = await response.json();
                  if (!response.ok) throw Error(result.error?.message ?? "Unable to save.");
                  router.refresh();
                } catch (cause) {
                  setError(cause instanceof Error ? cause.message : "Unable to save.");
                } finally {
                  setBusy(null);
                }
              }}
            >
              <strong data-ui-appearance={appearance}>
                Level {level.level_number} · {level.name}{" "}
                {current ? `· v${current.version}` : "· no default"}
              </strong>
              <Field appearance={appearance}>
                Annual days
                <TextInput appearance={appearance}
                  name="annual"
                  type="number"
                  min="0"
                  max="365"
                  step="0.5"
                  required
                  defaultValue={current?.annual_days ?? 0}
                />
              </Field>
              <Field appearance={appearance}>
                Sick days
                <TextInput appearance={appearance}
                  name="sick"
                  type="number"
                  min="0"
                  max="365"
                  step="0.5"
                  required
                  defaultValue={current?.sick_days ?? 0}
                />
              </Field>
              <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={busy !== null}>
                Save new version
              </Button>
            </form>
          );
        })}
      </div>
      {error && (
        <p data-ui-appearance={appearance} role="alert" className="error">
          {error}
        </p>
      )}
    </section>
  );
}
