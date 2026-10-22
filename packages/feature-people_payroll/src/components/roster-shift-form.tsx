"use client";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useState, type FormEvent } from "react";
import { addDays, rosterMonday, type RosterData, type RosterShift } from "@erpx/feature-people_payroll/lib/people-roster";
import legacyStyles from "@erpx/feature-people_payroll/components/roster.module.css";
import standardStyles from "@erpx/feature-people_payroll/components/roster-owner-workspace.module.css";
import { type UiAppearance } from "@erpx/ui/components/foundation";
function form(e: FormEvent<HTMLFormElement>) {
  e.preventDefault();
  return new FormData(e.currentTarget);
}
function field(f: FormData, name: string) {
  return String(f.get(name) ?? "");
}

export function RosterShiftForm({ appearance = "legacy",
  editor,
  data,
  busy,
  submit,
  cancelShift,
}: ({
  editor: { kind: "shift"; shift?: RosterShift; date?: string };
  data: RosterData;
  busy: boolean;
  submit: (input: unknown) => Promise<void>;
  cancelShift: (s: RosterShift) => void;
}) & { appearance?: UiAppearance }) {
  const styles = appearance === "standard" ? standardStyles : legacyStyles;
  const s = editor.shift,
    first = data.templates.find((t) => t.active),
    week = data.week!;
  const [templateId, setTemplateId] = useState(s?.template_id ?? first?.id ?? "");
  const [name, setName] = useState(s?.name ?? first?.name ?? "");
  const [start, setStart] = useState(s?.start_time ?? first?.start_time ?? "06:00");
  const [end, setEnd] = useState(s?.end_time ?? first?.end_time ?? "14:00");
  const [capacity, setCapacity] = useState(s?.capacity ?? first?.capacity ?? 2);
  const defaultDate =
    editor.date ??
    s?.local_date ??
    (rosterMonday() === week.week_start
      ? new Intl.DateTimeFormat("en-CA", {
          timeZone: week.timezone,
          year: "numeric",
          month: "2-digit",
          day: "2-digit",
        }).format(new Date(data.asOf))
      : week.week_start);
  return (
    <form data-ui-appearance={appearance}
      className={styles.form}
      onSubmit={(e) => {
        const f = form(e);
        void submit({
          action: "shift.save",
          weekId: week.id,
          expectedVersion: week.version,
          ...(s ? { id: s.id } : {}),
          ...(templateId ? { templateId } : {}),
          date: field(f, "date"),
          name,
          startTime: start,
          endTime: end,
          capacity,
          personIds: f.getAll("staff"),
          reason: field(f, "reason"),
        });
      }}
    >
      <Field appearance={appearance}>
        Shift template
        <Select appearance={appearance}
          value={templateId}
          onChange={(e) => {
            const t = data.templates.find((t) => t.id === e.target.value);
            setTemplateId(e.target.value);
            if (t) {
              setName(t.name);
              setStart(t.start_time);
              setEnd(t.end_time);
              setCapacity(t.capacity);
            }
          }}
        >
          <option data-ui-appearance={appearance} value="">Custom shift</option>
          {data.templates
            .filter((t) => t.active)
            .map((t) => (
              <option data-ui-appearance={appearance} key={t.id} value={t.id}>
                {t.name} · {t.start_time}–{t.end_time}
              </option>
            ))}
        </Select>
      </Field>
      <div data-ui-appearance={appearance} className={styles.twoFields}>
        <Field appearance={appearance}>
          Shift name
          <TextInput appearance={appearance}
            required
            maxLength={80}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        <Field appearance={appearance}>
          Date
          <TextInput appearance={appearance}
            name="date"
            type="date"
            required
            min={week.week_start}
            max={addDays(week.week_start, 6)}
            defaultValue={defaultDate}
          />
        </Field>
      </div>
      <div data-ui-appearance={appearance} className={styles.twoFields}>
        <Field appearance={appearance}>
          Start time
          <TextInput appearance={appearance}
            type="time"
            required
            value={start}
            onChange={(e) => setStart(e.target.value)}
          />
        </Field>
        <Field appearance={appearance}>
          End time
          <TextInput appearance={appearance} type="time" required value={end} onChange={(e) => setEnd(e.target.value)} />
        </Field>
      </div>
      <Field appearance={appearance}>
        Required staff
        <TextInput appearance={appearance}
          type="number"
          min="1"
          max="100"
          required
          value={capacity}
          onChange={(e) => setCapacity(Number(e.target.value))}
        />
      </Field>
      <fieldset data-ui-appearance={appearance} className={styles.staffPicker}>
        <legend data-ui-appearance={appearance}>Assign staff</legend>
        {data.people.map((p) => (
          <Field appearance={appearance} orientation="inline" className={styles.check} key={p.person_id}>
            <TextInput appearance={appearance}
              type="checkbox"
              name="staff"
              value={p.person_id}
              defaultChecked={data.assignments.some(
                (a) =>
                  a.shift_id === s?.id && a.person_id === p.person_id && a.state === "assigned",
              )}
            />
            {p.legal_name}
          </Field>
        ))}
        {!data.people.length && <p data-ui-appearance={appearance}>No eligible staff in this department.</p>}
      </fieldset>
      {week.state === "published" && (
        <Field appearance={appearance}>
          Reason for this change
          <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
        </Field>
      )}
      <div data-ui-appearance={appearance} className={styles.actions}>
        <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
          Save shift
        </Button>
        {s && (
          <Button appearance={appearance}
            variant="danger"
            type="button"
            className={styles.danger}
            disabled={busy}
            onClick={() => cancelShift(s)}
          >
            Cancel shift
          </Button>
        )}
      </div>
    </form>
  );
}
