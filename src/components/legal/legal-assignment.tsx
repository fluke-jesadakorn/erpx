"use client";
import type { LegalDetail } from "@/lib/legal/model";
import { useLegalPeople } from "./legal-person-select";
import s from "./legal.module.css";
export function LegalAssignment({ detail, call, command, busy }: { detail: LegalDetail; call: (path: string) => Promise<unknown>; command: (body: Record<string, unknown>) => Promise<boolean>; busy: boolean }) {
  const { people, error } = useLegalPeople(call, "owner", detail.item.id);
  return <form className={s.detail} onSubmit={event => { event.preventDefault(); void command({ action: "assign", ...Object.fromEntries(new FormData(event.currentTarget)) }); }}>
    <h3>Assign legal owner</h3>{error && <p role="alert">{error}</p>}<label className={s.field}>Legal owner<select name="ownerId" required defaultValue={detail.item.owner_id}>{people.map(person => <option key={person.id} value={person.id}>{person.name}</option>)}</select></label><label className={s.field}>Assignment reason<textarea name="reason" required minLength={3} /></label><button className={s.button} disabled={busy || !people.length}>Assign owner</button>
  </form>;
}
