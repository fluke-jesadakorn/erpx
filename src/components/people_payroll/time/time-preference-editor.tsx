"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useEffect, useState } from "react";
import { formatPreferredDateTime, validTimeZone, type TimePreference } from "@/lib/shared/ui-format";
import { useTimePreference } from "@/components/people_payroll/time/time-preference-context";
import { Card } from "@/components/ui/foundation";

const commonZones = [
  "Asia/Bangkok",
  "UTC",
  "Asia/Tokyo",
  "Asia/Singapore",
  "Europe/London",
  "Europe/Paris",
  "America/New_York",
  "America/Los_Angeles",
  "Australia/Sydney",
];

export function TimePreferenceEditor() {
  const { preference, systemZone, ready, setPreference } = useTimePreference();
  const [draftOverride, setDraftOverride] = useState<TimePreference | null>(null);
  const draft = draftOverride ?? preference;
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);
  async function save() {
    if (draft.mode === "manual" && (!draft.timeZone || !validTimeZone(draft.timeZone))) {
      setMessage("Choose a valid IANA timezone.");
      return;
    }
    setBusy(true);
    setMessage("");
    try {
      const response = await fetch("/api/v1/account/time-preference", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ...draft,
          timeZone: draft.mode === "system" ? null : draft.timeZone,
        }),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to save time settings.");
      setPreference(body.data);
      setDraftOverride(null);
      setMessage("Time settings saved.");
      if (typeof BroadcastChannel !== "undefined") {
        const channel = new BroadcastChannel("erpx-time-preference");
        channel.postMessage("updated");
        channel.close();
      }
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Unable to save time settings.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel" aria-label="Time and display settings">
      <h2>Time & display</h2>
      <p className="muted">
        Times use English dates and show their UTC offset. Work policies keep their own timezone.
      </p>
      <div className="profile-time-fields">
        <Field appearance="standard">
          Timezone source
          <Select appearance="standard"
            aria-label="Timezone source"
            disabled={!ready || busy}
            value={draft.mode}
            onChange={(event) =>
              setDraftOverride({
                ...draft,
                mode: event.target.value as TimePreference["mode"],
                timeZone: event.target.value === "system" ? null : (draft.timeZone ?? systemZone),
              })
            }
          >
            <option value="system">System · {systemZone}</option>
            <option value="manual">Manual</option>
          </Select>
        </Field>
        {draft.mode === "manual" && (
          <Field appearance="standard">
            Timezone
            <TextInput appearance="standard"
              aria-label="Timezone"
              disabled={busy}
              list="profile-timezones"
              value={draft.timeZone ?? ""}
              onChange={(event) => setDraftOverride({ ...draft, timeZone: event.target.value })}
              placeholder="Asia/Bangkok"
            />
            <datalist id="profile-timezones">
              {commonZones.map((zone) => (
                <option key={zone} value={zone} />
              ))}
            </datalist>
          </Field>
        )}
        <Field appearance="standard">
          Clock format
          <Select appearance="standard"
            aria-label="Clock format"
            disabled={!ready || busy}
            value={draft.hourCycle}
            onChange={(event) =>
              setDraftOverride({
                ...draft,
                hourCycle: event.target.value as TimePreference["hourCycle"],
              })
            }
          >
            <option value="h23">24 hour</option>
            <option value="h12">12 hour</option>
          </Select>
        </Field>
      </div>
      <p className="profile-time-preview">
        <span>Preview now</span>
        <strong>
          {ready &&
          now &&
          (draft.mode === "system" || (!!draft.timeZone && validTimeZone(draft.timeZone)))
            ? formatPreferredDateTime(now, draft, systemZone)
            : "…"}
        </strong>
      </p>
      <Button appearance="standard" variant="primary" type="button" disabled={!ready || busy} onClick={() => void save()}>
        {busy ? "Saving…" : "Save time settings"}
      </Button>
      {message && <InlineNotice tone="info" role="status">{message}</InlineNotice>}
    </Card>
  );
}
