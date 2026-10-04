"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import { StaffPicker, useStaffCommand, type StaffOptions } from "./staff-work-controls";
import { Card, FormLayout } from "@/components/ui/foundation";
export function StaffSettingsControls({ base, options }: { base: string; options: StaffOptions }) {
  const { send, busy, error } = useStaffCommand(base),
    [head, setHead] = useState("");
  return (
    <>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Company level catalog</h2>
        <p className="muted">
          Level 1 is most senior; Level 6 is staff. Explicitly map staff in People. Legacy ranks
          remain for historical workflows. Levels do not grant access.
        </p>
        {[...options.levels, null].map((level) => (
          <FormDisclosure appearance="standard" key={level?.id ?? "new"}>
            <summary>
              {level
                ? `${level.code} · ${level.name} · ${level.level_number ? `Level ${level.level_number}` : `legacy rank ${level.rank}`} · ${level.active ? "active" : "inactive"}`
                : "Add staff level"}
            </summary>
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send(`staff/levels${level ? `/${level.id}` : ""}`, {
                  code: f.get("code"),
                  name: f.get("name"),
                  rank: Number(f.get("rank")),
                  ...(level?.level_number ? { levelNumber: level.level_number } : {}),
                  active: f.get("active") === "on",
                  expectedVersion: level?.version ?? 0,
                });
              }}
            >
              <Field appearance="standard">
                Code
                <TextInput appearance="standard" name="code" required maxLength={30} defaultValue={level?.code ?? ""} />
              </Field>
              <Field appearance="standard">
                Name
                <TextInput appearance="standard" name="name" required maxLength={100} defaultValue={level?.name ?? ""} />
              </Field>
              <Field appearance="standard">
                Legacy rank (historical workflows)
                <TextInput appearance="standard"
                  name="rank"
                  type="number"
                  required
                  min={-10000}
                  max={10000}
                  defaultValue={level?.rank ?? 1}
                />
              </Field>
              <Field appearance="standard" orientation="inline" className="staff-check">
                <TextInput appearance="standard"
                  name="active"
                  type="checkbox"
                  defaultChecked={level ? !!level.active : true}
                />
                Active
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Save level
              </Button>
            </FormLayout>
          </FormDisclosure>
        ))}
      </Card>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Named department heads</h2>
        {options.departments.map((d) => (
          <p key={d.id}>
            <strong>{d.display_name}</strong> · {d.head_name}
          </p>
        ))}
        <FormDisclosure appearance="standard">
          <summary>Change department head</summary>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                department = options.departments.find((d) => d.id === f.get("departmentId")),
                person = options.people.find((p) => p.user_id === head);
              if (!department || !person) return;
              void send(`staff/departments/${department.id}/head`, {
                personId: person.person_id,
                expectedVersion: department.version ?? 0,
                reason: f.get("reason"),
              });
            }}
          >
            <Field appearance="standard">
              Department
              <Select appearance="standard" name="departmentId" required>
                <option value="">Choose department</option>
                {options.departments.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.display_name}
                  </option>
                ))}
              </Select>
            </Field>
            <StaffPicker appearance="standard"
              label="Named department head"
              value={head}
              onChange={setHead}
              options={options}
            />
            <Field appearance="standard">
              Configuration reason
              <TextInput appearance="standard" name="reason" required minLength={3} maxLength={1000} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Save department head
            </Button>
          </form>
        </FormDisclosure>
      </Card>
    </>
  );
}
