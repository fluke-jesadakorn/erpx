"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { payrollErrorMessage, type PayrollApiError } from "@/lib/payroll-errors";
type Schedule = {
    id: string;
    frequency: string;
    timezone: string;
    currency: string;
    enabled: boolean;
    payDelayDays: number;
    anchorDate: string | null;
    version: number;
};
export function PayrollScheduleSettings({ base, schedules, names }: {
    base: string;
    userId: string;
    schedules: Schedule[];
    names: Record<string, string>;
}) {
    return <section className="card"><h2>{"Scheduled pay runs"}</h2><p className="muted">{"Drafts and previews are prepared automatically. Approval, posting, and payment remain manual."}</p>
    {schedules.length ? schedules.map(schedule => <ScheduleForm key={schedule.id} base={base} schedule={schedule} name={names[schedule.id] ?? schedule.id}/>) : <p>{"Create a pay group first."}</p>}
  </section>;
}
function ScheduleForm({ base, schedule, name }: {
    base: string;
    schedule: Schedule;
    name: string;
}) {
    const router = useRouter();
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState("");
    const [enabled, setEnabled] = useState(schedule.enabled);
    const attempt = useRef<{
        body: string;
        key: string;
    } | null>(null);
    const frequency = schedule.frequency;
    async function save(form: HTMLFormElement) {
        const data = new FormData(form);
        const input = { enabled, payDelayDays: Number(data.get("payDelayDays")), anchorDate: String(data.get("anchorDate") ?? "") || null, expectedVersion: schedule.version };
        const body = JSON.stringify(input), path = `pay-groups/${encodeURIComponent(schedule.id)}/schedule`;
        if (attempt.current?.body !== body)
            attempt.current = { body, key: crypto.randomUUID() };
        setBusy(true);
        setError("");
        try {
            const response = await workFetch(`/api/v1${base}/people/${path}`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key }, body });
            const result = await response.json() as {
                error?: PayrollApiError;
            };
            if (!response.ok)
                throw result.error ?? { message: "Could not save the schedule." };
            attempt.current = null;
            router.refresh();
        }
        catch (cause) {
            setError(payrollErrorMessage(cause, "Could not save the schedule."));
        }
        finally {
            setBusy(false);
        }
    }
    return <form className="business-form" onSubmit={event => { event.preventDefault(); void save(event.currentTarget); }}>
    <h3>{name} · {schedule.currency} · {frequency}</h3>
    {error && <p role="alert">{error}</p>}
    <label><input type="checkbox" checked={enabled} onChange={event => setEnabled(event.currentTarget.checked)}/> {"Create scheduled drafts"}</label>
    <label>{"Days after period end"}<input name="payDelayDays" type="number" min="1" max="45" step="1" required defaultValue={schedule.payDelayDays}/></label>
    {["weekly", "fortnightly"].includes(schedule.frequency) && <label>{"Schedule anchor date"}<input name="anchorDate" type="date" required={enabled} defaultValue={schedule.anchorDate ?? ""}/></label>}
    <p className="muted">{"Timezone"}: {schedule.timezone} · {"Dates use Gregorian years."}</p>
    <button className="button secondary" disabled={busy}>{busy ? ("Saving…") : ("Save schedule")}</button>
  </form>;
}
