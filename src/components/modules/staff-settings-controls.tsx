"use client";
import { useState } from "react";
import {
  StaffPicker,
  useStaffCommand,
  type StaffOptions,
} from "./staff-work-controls";
export function StaffSettingsControls({
  base,
  options,
}: {
  base: string;
  options: StaffOptions;
}) {
  const { send, busy, error } = useStaffCommand(base),
    [head, setHead] = useState("");
  return (
    <>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <section className="card">
        <h2>Company level catalog</h2>
        <p className="muted">
          Level 1 is most senior; Level 6 is staff. Explicitly map staff in People. Legacy ranks remain for historical workflows. Levels do not grant access.
        </p>
        {[...options.levels, null].map((level) => (
          <details key={level?.id ?? "new"}>
            <summary>
              {level
                ? `${level.code} · ${level.name} · ${level.level_number ? `Level ${level.level_number}` : `legacy rank ${level.rank}`} · ${level.active ? "active" : "inactive"}`
                : "Add staff level"}
            </summary>
            <form
              className="business-form"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send(`staff/levels${level ? `/${level.id}` : ""}`, {
                  code: f.get("code"),
                  name: f.get("name"),
                  rank: Number(f.get("rank")),
                  ...(level?.level_number?{levelNumber:level.level_number}:{}),
                  active: f.get("active") === "on",
                  expectedVersion: level?.version ?? 0,
                });
              }}
            >
              <label>
                Code
                <input
                  name="code"
                  required
                  maxLength={30}
                  defaultValue={level?.code ?? ""}
                />
              </label>
              <label>
                Name
                <input
                  name="name"
                  required
                  maxLength={100}
                  defaultValue={level?.name ?? ""}
                />
              </label>
              <label>
                Legacy rank (historical workflows)
                <input
                  name="rank"
                  type="number"
                  required
                  min={-10000}
                  max={10000}
                  defaultValue={level?.rank ?? 1}
                />
              </label>
              <label className="staff-check">
                <input
                  name="active"
                  type="checkbox"
                  defaultChecked={level ? !!level.active : true}
                />
                Active
              </label>
              <button className="button" disabled={busy}>
                Save level
              </button>
            </form>
          </details>
        ))}
      </section>
      <section className="card">
        <h2>Named department heads</h2>
        {options.departments.map((d) => (
          <p key={d.id}>
            <strong>{d.display_name}</strong> · {d.head_name}
          </p>
        ))}
        <details>
          <summary>Change department head</summary>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                department = options.departments.find(
                  (d) => d.id === f.get("departmentId"),
                ),
                person = options.people.find((p) => p.user_id === head);
              if (!department || !person) return;
              void send(`staff/departments/${department.id}/head`, {
                personId: person.person_id,
                expectedVersion: department.version ?? 0,
                reason: f.get("reason"),
              });
            }}
          >
            <label>
              Department
              <select name="departmentId" required>
                <option value="">Choose department</option>
                {options.departments.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.display_name}
                  </option>
                ))}
              </select>
            </label>
            <StaffPicker
              label="Named department head"
              value={head}
              onChange={setHead}
              options={options}
            />
            <label>
              Configuration reason
              <input name="reason" required minLength={3} maxLength={1000} />
            </label>
            <button className="button" disabled={busy}>
              Save department head
            </button>
          </form>
        </details>
      </section>
    </>
  );
}
