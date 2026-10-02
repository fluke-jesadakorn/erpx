"use client";
import { useEffect, useState } from "react";
import s from "./legal.module.css";

export type LegalPerson = { id: string; name: string; role?: string; roles?: string[] };
export function useLegalPeople(call: (path: string) => Promise<unknown>, purpose: string, itemId?: string) {
  const [people, setPeople] = useState<LegalPerson[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    const query = new URLSearchParams({ purpose, ...(itemId ? { itemId } : {}) });
    call(`/people?${query}`).then(value => {
      if (active) { setPeople(Array.from(new Map((value as LegalPerson[]).map(person => [person.id, person])).values())); setError(""); }
    }).catch(error => { if (active) setError(error.message); });
    return () => { active = false; };
  }, [call, purpose, itemId]);
  return { people, error };
}

export function LegalPeopleSequence({ label, name, people, values, onChange }: {
  label: string; name: string; people: LegalPerson[]; values: string[]; onChange: (values: string[]) => void;
}) {
  return <fieldset className={s.detail}>
    <legend>{label}</legend>
    {values.map((value, index) => <div className={s.actions} key={index}>
      <label className={s.field}>{label} {index + 1}
        <select name={`${name}:${index}`} value={value} required onChange={event => onChange(values.map((entry, at) => at === index ? event.target.value : entry))}>
          <option value="">Choose a person</option>
          {people.filter(person => person.id === value || !values.includes(person.id)).map(person => <option key={person.id} value={person.id}>{person.name}</option>)}
        </select>
      </label>
      {values.length > 1 && <button className={s.button} type="button" onClick={() => onChange(values.filter((_, at) => at !== index))}>Remove</button>}
    </div>)}
    <button className={s.button} type="button" disabled={values.length >= 10 || values.length >= people.length} onClick={() => onChange([...values, ""])}>Add {label.toLowerCase()}</button>
  </fieldset>;
}
