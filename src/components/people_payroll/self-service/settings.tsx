"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRouter } from "@/lib/shared/destination-router";
import { useRef, useState, useSyncExternalStore, type FormEvent } from "react";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { leaveAdministration } from "@/server/modules/people_payroll/public/leave-self-service";
import styles from "@/components/people_payroll/self-service/self-service.module.css";

type Data = Awaited<ReturnType<typeof leaveAdministration>>;
type Action =
  "type" | "calendar" | "holiday" | "assignment" | "adjustment" | "eligibility" | "assignment_end";
const pageSize = 10;
const subscribeHydration = () => () => {};
const clientReady = () => true;
const serverReady = () => false;

export function SelfServiceSettings({ base, data }: { base: string; data: Data }) {
  const router = useRouter();
  const [action, setAction] = useState<Action | null>(null);
  const [endingId, setEndingId] = useState("");
  const [eligibilityId, setEligibilityId] = useState("");
  const [page, setPage] = useState(0);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const ready = useSyncExternalStore(subscribeHydration, clientReady, serverReady);
  const attempt = useRef<{ body: string; key: string } | null>(null);
  const activeAssignments = data.assignments.filter((a) => !a.effective_to).length;
  const selectedAssignment = data.assignments.find((a) => a.id === endingId);
  const unavailable = (id: Action | "policy") =>
    (id === "assignment" && (!data.policies.length || !data.engagements.length)) ||
    (id === "adjustment" && (!data.types.length || !data.engagements.length)) ||
    (id === "eligibility" && !data.engagements.length);
  function open(next: Action, id = "") {
    setAction(next);
    setEndingId(id);
    setError("");
    setMessage("");
  }
  function close() {
    if (!busy) {
      setAction(null);
      setError("");
    }
  }
  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!action) return;
    const f = new FormData(event.currentTarget);
    const value = (k: string) => String(f.get(k) ?? "");
    const number = (k: string) => Number(f.get(k) ?? 0);
    let input: unknown;
    switch (action) {
      case "type":
        input = {
          resource: "type",
          code: value("code"),
          name: value("name"),
          description: value("description"),
          tracked: f.has("tracked"),
          evidenceAfterUnits: value("evidence") ? number("evidence") : null,
          evidenceNote: value("evidenceNote"),
        };
        break;
      case "calendar":
        input = { resource: "calendar", name: value("name"), timezone: value("timezone") };
        break;
      case "holiday":
        input = {
          resource: "holiday",
          calendarId: value("calendar"),
          date: value("date"),
          name: value("name"),
        };
        break;
      case "assignment":
        input = {
          resource: "assignment",
          engagementId: value("engagement"),
          policyVersionId: value("policy"),
          calendarId: value("calendar") || null,
          effectiveFrom: value("from"),
          effectiveTo: value("to") || null,
        };
        break;
      case "adjustment":
        input = {
          resource: "adjustment",
          engagementId: value("engagement"),
          leaveTypeId: value("type"),
          units: number("units"),
          effectiveDate: value("date"),
          note: value("note"),
        };
        break;
      case "assignment_end":
        input = {
          resource: "assignment_end",
          id: endingId,
          expectedVersion: selectedAssignment?.version ?? 0,
          effectiveTo: value("to"),
        };
        break;
      case "eligibility":
        input = {
          engagementId: value("engagement"),
          expectedVersion:
            data.eligibilities.find((e) => e.engagement_id === value("engagement"))?.version ?? 0,
          eligible: value("eligible") === "true",
          topLevel: f.has("topLevel"),
          reason: value("note"),
        };
        break;
    }
    const body = JSON.stringify(input);
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(
        `/api/v1${base}/${action === "eligibility" ? "overtime-requests/eligibility" : "leave/admin"}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body,
        },
      );
      const result = await response.json();
      if (!response.ok) throw Error(result.error?.message ?? "Unable to save settings.");
      attempt.current = null;
      setAction(null);
      setMessage("Saved to server.");
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to save settings.");
    } finally {
      setBusy(false);
    }
  }
  const people = (
    <Field appearance="standard">
      Engagement
      <Select appearance="standard"
        name="engagement"
        required
        value={action === "eligibility" ? eligibilityId : undefined}
        onChange={action === "eligibility" ? (e) => setEligibilityId(e.target.value) : undefined}
      >
        <option value="">Choose staff</option>
        {data.engagements.map((e) => (
          <option key={e.id} value={e.id}>
            {e.name} · {e.worker_type}
          </option>
        ))}
      </Select>
    </Field>
  );
  const types = (
    <Field appearance="standard">
      Leave type
      <Select appearance="standard" name="type" required>
        <option value="">Choose a type</option>
        {data.types.map((t) => (
          <option key={t.id} value={t.id}>
            {t.name}
          </option>
        ))}
      </Select>
    </Field>
  );
  const calendars = (
    <Field appearance="standard">
      Holiday calendar
      <Select appearance="standard" name="calendar">
        <option value="">No calendar</option>
        {data.calendars.map((c) => (
          <option key={c.id} value={c.id}>
            {c.name}
          </option>
        ))}
      </Select>
    </Field>
  );
  const actions: {
    id: Action | "policy";
    title: string;
    detail: string;
    count: string;
    label: string;
  }[] = [
    {
      id: "type",
      title: "Leave types & evidence",
      detail: "Define available leave and the evidence required.",
      count: `${data.types.length} types`,
      label: "Create leave type",
    },
    {
      id: "policy",
      title: "Policy versions",
      detail: "Set workdays, entitlement grants, and accruals.",
      count: `${data.policies.length} versions`,
      label: "Create policy",
    },
    {
      id: "calendar",
      title: "Holiday calendars",
      detail: data.calendars.length
        ? "Maintain calendars and dated holidays."
        : "Create a calendar before adding holidays.",
      count: `${data.calendars.length} calendars`,
      label: "Create calendar",
    },
    {
      id: "assignment",
      title: "Policy assignments",
      detail: "Assign a policy version to an active engagement.",
      count: `${activeAssignments} active`,
      label: "Assign policy",
    },
    {
      id: "adjustment",
      title: "Entitlement adjustments",
      detail: "Record a dated adjustment and reason.",
      count: "Manual entry",
      label: "Record adjustment",
    },
    {
      id: "eligibility",
      title: "Overtime eligibility",
      detail: "Review who may request overtime.",
      count: `${data.eligibilities.length} decisions`,
      label: "Set eligibility",
    },
  ];
  return (
    <div className={styles.settingsStack}>
      <aside className={styles.policyNote}>
        <strong>Policy model</strong>
        <span>
          Policies belong to the Company and are assigned to engagements. Positions organize work;
          they do not grant entitlement or approval authority.
        </span>
      </aside>
      {message && (
        <InlineNotice tone="info" className={styles.feedback} role="status">
          {message}
        </InlineNotice>
      )}
      <div className={styles.settingsList}>
        {actions.map((item) => (
          <section className={styles.settingsRow} key={item.id}>
            <div>
              <h2>{item.title}</h2>
              <p>{item.detail}</p>
              <small>{item.count}</small>
            </div>
            {item.id === "policy" ? (
              <UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/people/settings/leave-policy/new`)}>
                {item.label}
              </UiActionLink>
            ) : (
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={!ready || unavailable(item.id)}
                onClick={() => open(item.id as Action)}
              >
                {item.label}
              </Button>
            )}
            {item.id === "calendar" && (
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={!ready || !data.calendars.length}
                onClick={() => open("holiday")}
              >
                Add holiday
              </Button>
            )}
          </section>
        ))}
      </div>
      <section className={styles.assignmentList} aria-labelledby="policy-assignment-heading">
        <div className={styles.assignmentHead}>
          <div>
            <h2 id="policy-assignment-heading">Current policy assignments</h2>
            <p>
              {data.assignments.length} total · {activeAssignments} active
            </p>
          </div>
        </div>
        {data.assignments.length ? (
          data.assignments.slice(page * pageSize, (page + 1) * pageSize).map((a) => (
            <div className={styles.assignmentRow} data-assignment-row key={a.id}>
              <div>
                <strong>{a.name}</strong>
                <span>
                  {a.policy} · {a.effective_from} — {a.effective_to ?? "ongoing"}
                </span>
              </div>
              {!a.effective_to && (
                <Button appearance="standard"
                  variant="secondary"
                  type="button"
                  disabled={!ready}
                  onClick={() => open("assignment_end", a.id)}
                >
                  End assignment
                </Button>
              )}
            </div>
          ))
        ) : (
          <p>No policy assignments yet.</p>
        )}
        {data.assignments.length > pageSize && (
          <nav className={styles.pagination} aria-label="Policy assignment pages">
            <Button appearance="standard"
              variant="secondary"
              type="button"
              disabled={!ready || page === 0}
              onClick={() => setPage((p) => p - 1)}
            >
              Previous
            </Button>
            <span>
              Page {page + 1} of {Math.ceil(data.assignments.length / pageSize)}
            </span>
            <Button appearance="standard"
              variant="secondary"
              type="button"
              disabled={!ready || (page + 1) * pageSize >= data.assignments.length}
              onClick={() => setPage((p) => p + 1)}
            >
              Next
            </Button>
          </nav>
        )}
      </section>
      <Dialog appearance="standard"
        open={!!action}
        onClose={close}
        title={
          {
            type: "Create leave type",
            calendar: "Create holiday calendar",
            holiday: "Add holiday",
            assignment: "Assign policy to staff",
            adjustment: "Record entitlement adjustment",
            eligibility: "Set overtime eligibility",
            assignment_end: "End policy assignment",
          }[action ?? "type"]
        }
        presentation="form-drawer"
      >
        <form className={styles.form} onSubmit={(e) => void save(e)}>
          {action === "type" && (
            <>
              <Field appearance="standard">
                Code
                <TextInput appearance="standard" name="code" required />
              </Field>
              <Field appearance="standard">
                Name
                <TextInput appearance="standard" name="name" required />
              </Field>
              <Field appearance="standard">
                Description
                <TextArea appearance="standard" name="description" />
              </Field>
              <Field appearance="standard" orientation="inline">
                <TextInput appearance="standard" type="checkbox" name="tracked" defaultChecked />
                Track entitlement
              </Field>
              <Field appearance="standard">
                Evidence required after (days)
                <TextInput appearance="standard" type="number" name="evidence" min="0.5" step="0.5" />
              </Field>
              <Field appearance="standard">
                Evidence guidance
                <TextArea appearance="standard" name="evidenceNote" />
              </Field>
            </>
          )}
          {action === "calendar" && (
            <>
              <Field appearance="standard">
                Calendar name
                <TextInput appearance="standard" name="name" required />
              </Field>
              <Field appearance="standard">
                Timezone
                <TextInput appearance="standard" name="timezone" defaultValue="Asia/Bangkok" required />
              </Field>
            </>
          )}
          {action === "holiday" && (
            <>
              <Field appearance="standard">
                Holiday calendar
                <Select appearance="standard" name="calendar" required>
                  <option value="">Choose a calendar</option>
                  {data.calendars.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Holiday date
                <TextInput appearance="standard" name="date" type="date" required />
              </Field>
              <Field appearance="standard">
                Holiday name
                <TextInput appearance="standard" name="name" required />
              </Field>
            </>
          )}
          {action === "assignment" && (
            <>
              {people}
              <Field appearance="standard">
                Policy version
                <Select appearance="standard" name="policy" required>
                  <option value="">Choose a policy</option>
                  {data.policies.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name} · v{p.version} · {p.effective_from}
                    </option>
                  ))}
                </Select>
              </Field>
              {calendars}
              <div className={styles.fields}>
                <Field appearance="standard">
                  Effective from
                  <TextInput appearance="standard" type="date" name="from" required />
                </Field>
                <Field appearance="standard">
                  Effective to
                  <TextInput appearance="standard" type="date" name="to" />
                </Field>
              </div>
            </>
          )}
          {action === "adjustment" && (
            <>
              {people}
              {types}
              <Field appearance="standard">
                Adjustment units
                <TextInput appearance="standard" type="number" step="0.5" name="units" required />
              </Field>
              <Field appearance="standard">
                Effective date
                <TextInput appearance="standard" name="date" type="date" required />
              </Field>
              <Field appearance="standard">
                Reason
                <TextArea appearance="standard" name="note" required />
              </Field>
            </>
          )}
          {action === "eligibility" && (
            <>
              {people}
              <div key={eligibilityId} className={styles.form}>
                <Field appearance="standard">
                  Eligibility
                  <Select appearance="standard"
                    name="eligible"
                    defaultValue={String(
                      data.eligibilities.find((e) => e.engagement_id === eligibilityId)?.eligible ??
                        true,
                    )}
                  >
                    <option value="true">Eligible</option>
                    <option value="false">Ineligible</option>
                  </Select>
                </Field>
                <Field appearance="standard">
                  Reason
                  <TextArea appearance="standard" name="note" required minLength={3} />
                </Field>
                <Field appearance="standard" orientation="inline">
                  <TextInput appearance="standard"
                    type="checkbox"
                    name="topLevel"
                    defaultChecked={
                      data.eligibilities.find((e) => e.engagement_id === eligibilityId)
                        ?.top_level ?? false
                    }
                  />
                  Confirmed top-level engagement (no reporting manager)
                </Field>
                <p className={styles.muted}>
                  Contractors require an explicit eligible decision. Manager and independent HR
                  reviews still apply.
                </p>
              </div>
            </>
          )}
          {action === "assignment_end" && selectedAssignment && (
            <>
              <p>
                {selectedAssignment.name} · {selectedAssignment.policy}
              </p>
              <Field appearance="standard">
                End assignment on
                <TextInput appearance="standard" type="date" name="to" min={selectedAssignment.effective_from} required />
              </Field>
            </>
          )}
          {error && (
            <InlineNotice tone="danger" className={styles.error} role="alert">
              {error}
            </InlineNotice>
          )}
          <div className={styles.actions}>
            <Button appearance="standard" variant="secondary" type="button" onClick={close}>
              Cancel
            </Button>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              {busy ? "Saving…" : "Save"}
            </Button>
          </div>
        </form>
      </Dialog>
    </div>
  );
}
