"use client";
import { useState, type FormEvent } from "react";
import {
  addDays,
  rosterMonday,
  type RosterData,
  type RosterShift,
} from "@/lib/people-roster";
import styles from "./roster.module.css";
function form(e: FormEvent<HTMLFormElement>) {
  e.preventDefault();
  return new FormData(e.currentTarget);
}
function field(f: FormData, name: string) {
  return String(f.get(name) ?? "");
}

export function RosterShiftForm({
  editor,
  data,
  busy,
  submit,
  cancelShift,
}: {
  editor: { kind: "shift"; shift?: RosterShift; date?: string };
  data: RosterData;
  busy: boolean;
  submit: (input: unknown) => Promise<void>;
  cancelShift: (s: RosterShift) => void;
}) {
  const s = editor.shift,
    first = data.templates.find((t) => t.active),
    week = data.week!;
  const [templateId, setTemplateId] = useState(
    s?.template_id ?? first?.id ?? "",
  );
  const [name, setName] = useState(s?.name ?? first?.name ?? "");
  const [start, setStart] = useState(
    s?.start_time ?? first?.start_time ?? "06:00",
  );
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
    <form
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
      <label>
        Shift template
        <select
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
          <option value="">Custom shift</option>
          {data.templates
            .filter((t) => t.active)
            .map((t) => (
              <option key={t.id} value={t.id}>
                {t.name} · {t.start_time}–{t.end_time}
              </option>
            ))}
        </select>
      </label>
      <div className={styles.twoFields}>
        <label>
          Shift name
          <input
            required
            maxLength={80}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <label>
          Date
          <input
            name="date"
            type="date"
            required
            min={week.week_start}
            max={addDays(week.week_start, 6)}
            defaultValue={defaultDate}
          />
        </label>
      </div>
      <div className={styles.twoFields}>
        <label>
          Start time
          <input
            type="time"
            required
            value={start}
            onChange={(e) => setStart(e.target.value)}
          />
        </label>
        <label>
          End time
          <input
            type="time"
            required
            value={end}
            onChange={(e) => setEnd(e.target.value)}
          />
        </label>
      </div>
      <label>
        Required staff
        <input
          type="number"
          min="1"
          max="100"
          required
          value={capacity}
          onChange={(e) => setCapacity(Number(e.target.value))}
        />
      </label>
      <fieldset className={styles.staffPicker}>
        <legend>Assign staff</legend>
        {data.people.map((p) => (
          <label className={styles.check} key={p.person_id}>
            <input
              type="checkbox"
              name="staff"
              value={p.person_id}
              defaultChecked={data.assignments.some(
                (a) =>
                  a.shift_id === s?.id &&
                  a.person_id === p.person_id &&
                  a.state === "assigned",
              )}
            />
            {p.legal_name}
          </label>
        ))}
        {!data.people.length && <p>No eligible staff in this department.</p>}
      </fieldset>
      {week.state === "published" && (
        <label>
          Reason for this change
          <textarea name="reason" required minLength={3} maxLength={1000} />
        </label>
      )}
      <div className={styles.actions}>
        <button className={styles.primary} disabled={busy}>
          Save shift
        </button>
        {s && (
          <button
            type="button"
            className={styles.danger}
            disabled={busy}
            onClick={() => cancelShift(s)}
          >
            Cancel shift
          </button>
        )}
      </div>
    </form>
  );
}
