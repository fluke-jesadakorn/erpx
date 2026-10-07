"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea } from "@/components/ui/foundation";

import type { LegalDetail } from "@/server/modules/legal/public/rules/model";
import { useLegalPeople } from "./legal-person-select";
import s from "./legal.module.css";
export function LegalAssignment({
  detail,
  call,
  command,
  busy,
}: {
  detail: LegalDetail;
  call: (path: string) => Promise<unknown>;
  command: (body: Record<string, unknown>) => Promise<boolean>;
  busy: boolean;
}) {
  const { people, error } = useLegalPeople(call, "owner", detail.item.id);
  return (
    <form
      className={s.detail}
      onSubmit={(event) => {
        event.preventDefault();
        void command({
          action: "assign",
          ...Object.fromEntries(new FormData(event.currentTarget)),
        });
      }}
    >
      <h3>Assign legal owner</h3>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <Field appearance="standard" className={s.field}>
        Legal owner
        <Select appearance="standard" name="ownerId" required defaultValue={detail.item.owner_id}>
          {people.map((person) => (
            <option key={person.id} value={person.id}>
              {person.name}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance="standard" className={s.field}>
        Assignment reason
        <TextArea appearance="standard" name="reason" required minLength={3} />
      </Field>
      <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy || !people.length}>
        Assign owner
      </Button>
    </form>
  );
}
