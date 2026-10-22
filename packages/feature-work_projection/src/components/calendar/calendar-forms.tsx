"use client";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useState, type FormEvent } from "react";
import { addDays } from "@erpx/feature-people_payroll/lib/people-roster";
import { localDateTimeToInstant } from "@erpx/feature-work_projection/lib/calendar-timeline";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import type { CalendarItem, CalendarHierarchy } from "@erpx/feature-work_projection/lib/calendar";
import styles from "@erpx/feature-work_projection/components/calendar/calendar.module.css";
export type CalendarEditor = {
  kind: "appointment" | "announcement" | "task";
  item?: CalendarItem;
  startLocal?: string;
  endLocal?: string;
};
function localTime(ms: number, zone: string) {
  const p = new Intl.DateTimeFormat("en-CA", {
    timeZone: zone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).formatToParts(new Date(ms));
  const get = (key: string) => p.find((x) => x.type === key)?.value;
  return `${get("year")}-${get("month")}-${get("day")}T${get("hour")}:${get("minute")}`;
}
export function CalendarForm({
  editor,
  hierarchy,
  date,
  selfPersonId,
  busy,
  send,
  close,
  accounts,
  api,
  initialParticipantIds,
  initialTimezone,
}: {
  editor: CalendarEditor;
  hierarchy: CalendarHierarchy;
  date: string;
  selfPersonId: string | null;
  busy: boolean;
  send: (value: unknown) => void;
  close: () => void;
  accounts?: { id: string; name: string; email: string }[];
  api: string;
  initialParticipantIds?: string[];
  initialTimezone?: string;
}) {
  const item = editor.item,
    people = hierarchy.people,
    zone = item?.timezone ?? initialTimezone ?? "Asia/Bangkok";
  const [allDay, setAllDay] = useState(item?.allDay ?? false);
  const [availability, setAvailability] = useState<
    { userId: string; startsAt: number; endsAt: number }[] | null
  >(null);
  const [availabilityZone, setAvailabilityZone] = useState(zone);
  const [availabilityMessage, setAvailabilityMessage] = useState("");
  const [checkingAvailability, setCheckingAvailability] = useState(false);
  async function checkAvailability(form: HTMLFormElement) {
    const fields = new FormData(form),
      ids = [
        ...new Set(
          [...fields.getAll("requiredUsers"), ...fields.getAll("optionalUsers")].map(String),
        ),
      ];
    if (!ids.length) {
      setAvailability(null);
      setAvailabilityMessage("Choose at least one attendee.");
      return;
    }
    try {
      const zone = String(fields.get("timezone") || "Asia/Bangkok");
      const start = allDay ? `${fields.get("startDate")}T00:00` : String(fields.get("start"));
      const end = allDay ? `${fields.get("endDate")}T00:00` : String(fields.get("end"));
      const from = localDateTimeToInstant(start, zone),
        to = localDateTimeToInstant(end, zone);
      if (to <= from) throw Error("Choose an end after the start.");
      const query = new URLSearchParams({ from: String(from), to: String(to) });
      ids.forEach((id) => query.append("userId", id));
      setCheckingAvailability(true);
      setAvailabilityMessage("");
      const response = await workFetch(`${api}/availability?${query}`);
      const data = await response.json();
      if (!response.ok) throw Error(data?.error?.message ?? "Availability is unavailable.");
      setAvailabilityZone(zone);
      setAvailability(data.intervals ?? []);
    } catch (error) {
      setAvailability(null);
      setAvailabilityMessage(
        error instanceof Error ? error.message : "Availability is unavailable.",
      );
    } finally {
      setCheckingAvailability(false);
    }
  }
  function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget),
      value = (name: string) => String(f.get(name) ?? ""),
      version = item ? { id: item.sourceId, expectedVersion: item.version } : {};
    if (editor.kind === "appointment")
      send({
        action: "appointment.save",
        ...version,
        title: value("title"),
        description: value("description"),
        organizerPersonId: value("organizer") || undefined,
        participantIds: f.getAll("participants").map(String),
        ...(f.get("allDay") === "on"
          ? { allDay: true, startDate: value("startDate"), endDate: value("endDate") }
          : { startLocal: value("start"), endLocal: value("end") }),
        attendees: f
          .getAll("requiredUsers")
          .map(String)
          .map((userId) => ({ userId, attendance: "required" }))
          .concat(
            f
              .getAll("optionalUsers")
              .map(String)
              .map((userId) => ({ userId, attendance: "optional" })),
          ),
        recurrence:
          value("frequency") === "none"
            ? null
            : {
                frequency: value("frequency"),
                intervalCount: Number(value("interval")),
                weekdays: [],
                endKind: value("endKind"),
                ...(value("endKind") === "count"
                  ? { occurrenceCount: Number(value("count")) }
                  : {}),
                ...(value("endKind") === "until" ? { untilDate: value("untilDate") } : {}),
              },
        timezone: value("timezone"),
        location: value("location"),
        meetingUrl: value("link"),
        private: f.get("private") === "on",
        guests: value("guests")
          .split("\n")
          .map((x) => x.trim())
          .filter(Boolean)
          .map((line) => {
            const [name, email = "", phone = ""] = line.split("|").map((x) => x.trim());
            return { name, email, phone };
          }),
      });
    else if (editor.kind === "announcement")
      send({
        action: "announcement.save",
        ...version,
        title: value("title"),
        content: value("content"),
        audienceId: value("audience"),
        startDate: value("startDate"),
        endDate: value("endDate"),
      });
    else
      send({
        action: "task.plan",
        ...version,
        dueDate: value("dueDate") || null,
        assigneeId: value("assignee") || null,
        responsibility: value("responsibility"),
      });
  }
  return (
    <form className={styles.form} onSubmit={submit}>
      {editor.kind === "appointment" && (
        <>
          <Field>
            Title
            <TextInput
              name="title"
              required
              maxLength={160}
              defaultValue={item?.title}
              data-autofocus
            />
          </Field>
          <div className={styles.formRow}>
            <Field>
              Organizer
              <Select
                name="organizer"
                defaultValue={item?.details?.organizerPersonId ?? selfPersonId ?? ""}
              >
                <option value="">Me (Company account)</option>
                {people
                  .filter((p) => p.status !== "ended")
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field>
              Timezone
              <TextInput name="timezone" required defaultValue={zone} list="calendar-timezones" />
            </Field>
          </div>
          <Field orientation="inline" className={styles.check}>
            <TextInput
              type="checkbox"
              name="allDay"
              checked={allDay}
              onChange={(e) => setAllDay(e.target.checked)}
            />
            All day
          </Field>
          {allDay ? (
            <div className={styles.formRow}>
              <Field>
                Starts
                <TextInput
                  name="startDate"
                  type="date"
                  required
                  defaultValue={item?.startDate ?? date}
                />
              </Field>
              <Field>
                Ends before
                <TextInput
                  name="endDate"
                  type="date"
                  required
                  defaultValue={item?.endDate ?? addDays(date, 1)}
                />
              </Field>
            </div>
          ) : (
            <div className={styles.formRow}>
              <Field>
                Starts
                <TextInput
                  name="start"
                  type="datetime-local"
                  step="900"
                  required
                  defaultValue={
                    editor.startLocal ??
                    (item?.startsAt ? localTime(item.startsAt, zone) : `${date}T09:00`)
                  }
                />
              </Field>
              <Field>
                Ends
                <TextInput
                  name="end"
                  type="datetime-local"
                  step="900"
                  required
                  defaultValue={
                    editor.endLocal ??
                    (item?.endsAt ? localTime(item.endsAt, zone) : `${date}T10:00`)
                  }
                />
              </Field>
            </div>
          )}
          <div className={styles.formRow}>
            <Field>
              Repeat
              <Select name="frequency" defaultValue={item?.recurrence?.frequency ?? "none"}>
                <option value="none">Does not repeat</option>
                <option value="daily">Daily</option>
                <option value="weekly">Weekly</option>
                <option value="monthly">Monthly</option>
                <option value="yearly">Yearly</option>
              </Select>
            </Field>
            <Field>
              Every
              <TextInput
                name="interval"
                type="number"
                min="1"
                max="100"
                defaultValue={item?.recurrence?.intervalCount ?? 1}
              />
            </Field>
            <Field>
              Ends
              <Select name="endKind" defaultValue={item?.recurrence?.endKind ?? "never"}>
                <option value="never">Never</option>
                <option value="count">After occurrences</option>
                <option value="until">On date</option>
              </Select>
            </Field>
            <Field>
              Occurrences
              <TextInput
                name="count"
                type="number"
                min="1"
                max="1000"
                defaultValue={item?.recurrence?.occurrenceCount ?? 10}
              />
            </Field>
            <Field>
              End date
              <TextInput
                name="untilDate"
                type="date"
                defaultValue={item?.recurrence?.untilDate ?? ""}
              />
            </Field>
          </div>
          <div className={styles.formRow}>
            <Field>
              Required attendees
              <Select
                name="requiredUsers"
                multiple
                defaultValue={
                  item?.details?.attendees
                    ?.filter((p) => p.attendance === "required")
                    .map((p) => p.userId) ?? []
                }
              >
                {(accounts ?? []).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} · {p.email}
                  </option>
                ))}
              </Select>
            </Field>
            <Field>
              Optional attendees
              <Select
                name="optionalUsers"
                multiple
                defaultValue={
                  item?.details?.attendees
                    ?.filter((p) => p.attendance === "optional")
                    .map((p) => p.userId) ?? []
                }
              >
                {(accounts ?? []).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} · {p.email}
                  </option>
                ))}
              </Select>
            </Field>
          </div>
          <div>
            <Button
              variant="plain"
              type="button"
              disabled={checkingAvailability}
              onClick={(event) => {
                const form = event.currentTarget.form;
                if (form) void checkAvailability(form);
              }}
            >
              {checkingAvailability ? "Checking availability…" : "Check attendee availability"}
            </Button>
            {availabilityMessage && <p role="status">{availabilityMessage}</p>}
            {availability && (
              <div role="status" aria-label="Selected attendees' busy times">
                <strong>
                  {availability.length ? "Busy times" : "Selected attendees are free for this time"}
                </strong>
                {availability.map((interval, index) => (
                  <p key={`${interval.userId}:${interval.startsAt}:${index}`}>
                    {accounts?.find((account) => account.id === interval.userId)?.name ??
                      "Attendee"}
                    :{" "}
                    {new Intl.DateTimeFormat("en", {
                      timeZone: availabilityZone,
                      month: "short",
                      day: "numeric",
                      hour: "2-digit",
                      minute: "2-digit",
                    }).format(interval.startsAt)}
                    –
                    {new Intl.DateTimeFormat("en", {
                      timeZone: availabilityZone,
                      hour: "2-digit",
                      minute: "2-digit",
                    }).format(interval.endsAt)}{" "}
                    · Busy
                  </p>
                ))}
              </div>
            )}
          </div>
          <Field>
            Internal participants
            <Select
              name="participants"
              multiple
              defaultValue={item?.participants.map((p) => p.id) ?? initialParticipantIds ?? []}
            >
              {people
                .filter((p) => p.status !== "ended")
                .map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
            </Select>
            <span className={styles.muted}>
              Select colleagues; the organizer is always included.
            </span>
          </Field>
          <Field>
            External guests
            <TextArea
              name="guests"
              placeholder="Name | email | phone (one guest per line)"
              defaultValue={item?.details?.guests
                ?.map((g) => `${g.name} | ${g.email} | ${g.phone}`)
                .join("\n")}
            />
          </Field>
          <div className={styles.formRow}>
            <Field>
              Location
              <TextInput name="location" maxLength={300} defaultValue={item?.details?.location} />
            </Field>
            <Field>
              Meeting link
              <TextInput name="link" type="url" defaultValue={item?.details?.meetingUrl} />
            </Field>
          </div>
          <Field>
            Notes
            <TextArea name="description" maxLength={3000} defaultValue={item?.description} />
          </Field>
          <Field orientation="inline" className={styles.check}>
            <TextInput
              name="private"
              type="checkbox"
              defaultChecked={item?.details?.private ?? true}
            />
            Private appointment
          </Field>
          <p>
            Participants and authorized appointment managers can see guest details. Availability
            viewers see Busy. Overlaps produce warnings; meetings during shifts are allowed.
          </p>
        </>
      )}
      {editor.kind === "announcement" && (
        <>
          <Field>
            Title
            <TextInput
              name="title"
              required
              maxLength={160}
              defaultValue={item?.title}
              data-autofocus
            />
          </Field>
          <Field>
            Audience
            <Select
              name="audience"
              required
              defaultValue={
                item?.departmentId ??
                item?.branchId ??
                hierarchy.nodes.find((n) => n.kind === "company")?.id
              }
            >
              {hierarchy.nodes
                .filter(
                  (n) =>
                    ["company", "branch", "department"].includes(n.kind) && n.inScope !== false,
                )
                .map((n) => (
                  <option key={n.id} value={n.id}>
                    {n.name} · {n.kind}
                  </option>
                ))}
            </Select>
          </Field>
          <Field>
            Announcement
            <TextArea name="content" required maxLength={5000} defaultValue={item?.description} />
          </Field>
          <div className={styles.formRow}>
            <Field>
              Visible from
              <TextInput
                name="startDate"
                type="date"
                required
                defaultValue={item?.startDate ?? date}
              />
            </Field>
            <Field>
              Visible through
              <TextInput
                name="endDate"
                type="date"
                required
                defaultValue={item?.endDate ? addDays(item.endDate, -1) : date}
              />
            </Field>
          </div>
          <p>
            {item?.status === "published"
              ? "Saving updates the published announcement in Calendar and Home."
              : "Save a draft, then publish it from the announcement details."}
          </p>
        </>
      )}
      {editor.kind === "task" && (
        <>
          <p>{item?.title}</p>
          <Field>
            Deadline (optional)
            <TextInput
              name="dueDate"
              type="date"
              defaultValue={item?.startDate ?? ""}
              data-autofocus
            />
          </Field>
          <Field>
            Responsible colleague
            <Select name="assignee" defaultValue={item?.details?.assigneeId ?? ""}>
              <option value="">Unassigned</option>
              {item?.details?.assigneeId &&
                !people.some((p) => p.userId === item.details?.assigneeId) && (
                  <option value={item.details.assigneeId}>{item.owner}</option>
                )}
              {people
                .filter((p) => p.userId && p.status !== "ended")
                .map((p) => (
                  <option key={p.id} value={p.userId!}>
                    {p.name}
                  </option>
                ))}
            </Select>
          </Field>
          <Field>
            Responsibility
            <TextArea
              name="responsibility"
              maxLength={300}
              defaultValue={item?.details?.responsibility}
              placeholder="What this colleague needs to prepare"
            />
          </Field>
          <p>
            {item?.details?.privileged
              ? "This task retains its Company IT restrictions."
              : "The assigned colleague can complete this checklist task."}{" "}
            Checklist completion does not grant access or verify directory work.
          </p>
        </>
      )}
      <datalist id="calendar-timezones">
        <option value="Asia/Bangkok" />
        <option value="UTC" />
        <option value="Asia/Singapore" />
        <option value="Europe/London" />
        <option value="America/New_York" />
      </datalist>
      <div className={styles.actions}>
        <Button
          variant="primary"
          type="button"
          className={styles.button}
          onClick={close}
          disabled={busy}
        >
          Cancel
        </Button>
        <Button clientManaged variant="primary" type="submit" className={styles.primary} disabled={busy}>
          {busy
            ? "Saving…"
            : editor.kind === "appointment"
              ? "Save appointment"
              : editor.kind === "announcement"
                ? "Save announcement"
                : "Save task plan"}
        </Button>
      </div>
    </form>
  );
}
