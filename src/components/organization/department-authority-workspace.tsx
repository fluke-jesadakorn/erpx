"use client";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Combobox } from "@/components/ui/popups";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import { type UiAppearance } from "@/components/ui/foundation";

type Person = { person_id: string; legal_name: string; position_id: string | null };
type Position = { id: string; title: string; department_id: string | null; is_ceo: number };
type Seat = {
  kind: "ceo" | "hr" | "head";
  scope_id: string;
  version: number;
  person_id: string | null;
  position_id: string | null;
  person_name?: string | null;
  position_title?: string | null;
  department_name?: string | null;
};
type Selector = { kind: "person"; personId: string } | { kind: "position"; positionId: string };
type Policy = { department_id: string; version: number; steps_json: Selector[] | string };
type Data = {
  seats: Seat[];
  history: {
    id: string;
    kind: string;
    scope_id: string;
    reason: string;
    provenance: string;
    appointed_by: string | null;
    created_at: number;
    version: number;
  }[];
  policies: Policy[];
  departments: { id: string; display_name: string }[];
  canAppointKinds: ("ceo" | "hr" | "head")[];
  canPublishDepartmentIds: string[];
};
type Props = { holdingSlug: string; initial: Data; people: Person[]; positions: Position[] };
const label = { ceo: "CEO", hr: "HR", head: "Department head" };
function validateFlowSteps(steps: Selector[]) {
  if (steps.length < 1 || steps.length > 20) return "Choose between 1 and 20 approval steps.";
  const unique = new Set(
    steps.map((step) =>
      step.kind === "person" ? `person:${step.personId}` : `position:${step.positionId}`,
    ),
  );
  return unique.size === steps.length ? null : "Choose each person or position only once.";
}
function FormContext({ appearance = "legacy", version, effect }: ({ version: number; effect: string }) & { appearance?: UiAppearance }) {
  return (
    <div data-ui-appearance={appearance} className="notice">
      <strong data-ui-appearance={appearance}>Current version {version}</strong>
      <p data-ui-appearance={appearance}>{effect}</p>
      <p data-ui-appearance={appearance} className="muted">
        Saving checks the current version and the selected people or positions. If it changed,
        reload and review before saving again.
      </p>
    </div>
  );
}
async function post(url: string, body: unknown) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
    body: JSON.stringify(body),
  });
  const result = await response.json();
  if (!response.ok)
    throw new Error(result?.error?.message ?? result?.message ?? "Could not save this change.");
  return result;
}
function AppointmentEditor({ appearance = "legacy",
  seat,
  people,
  positions,
  url,
  onSaved,
}: ({
  seat: Seat;
  people: Person[];
  positions: Position[];
  url: string;
  onSaved: () => Promise<void>;
}) & { appearance?: UiAppearance }) {
  const [personId, setPersonId] = useState(seat.person_id ?? "");
  const [positionId, setPositionId] = useState(seat.position_id ?? "");
  const [reason, setReason] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const choices = positions.filter((p) =>
    seat.kind === "ceo"
      ? !!p.is_ceo
      : seat.kind === "head"
        ? p.department_id === seat.scope_id
        : true,
  );
  return (
    <FormDisclosure appearance="standard"><summary>Edit appointment · {seat.department_name ?? label[seat.kind]}</summary>
    <form data-ui-appearance={appearance}
      className={appearance === "standard" ? "ui-panel" : "card"}
      onSubmit={async (event) => {
        event.preventDefault();
        setBusy(true);
        setMessage("");
        try {
          await post(url, {
            kind: seat.kind,
            departmentId: seat.kind === "head" ? seat.scope_id : null,
            positionId: positionId || null,
            personId: personId || null,
            expectedVersion: seat.version,
            reason,
          });
          await onSaved();
          setReason("");
          setMessage("Appointment saved.");
        } catch (error) {
          setMessage((error as Error).message);
        } finally {
          setBusy(false);
        }
      }}
    >
      <h3 data-ui-appearance={appearance}>
        {label[seat.kind]}
        {seat.department_name ? ` · ${seat.department_name}` : ""}
      </h3>
      <p data-ui-appearance={appearance} className="muted">Current: {seat.person_name ?? "Vacant"}</p>
      <FormContext appearance={appearance}
        version={seat.version}
        effect="This appointment applies to new decisions immediately. Existing submitted routes retain their recorded approvers."
      />
      <Field appearance={appearance}>
        Position{" "}
        <Select appearance={appearance} value={positionId} onChange={(event) => setPositionId(event.target.value)}>
          <option data-ui-appearance={appearance} value="">No position</option>
          {choices.map((p) => (
            <option data-ui-appearance={appearance} key={p.id} value={p.id}>
              {p.title}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance={appearance}>
        Holder{" "}
        <Select appearance={appearance} value={personId} onChange={(event) => setPersonId(event.target.value)}>
          <option data-ui-appearance={appearance} value="">Vacant</option>
          {people.map((p) => (
            <option data-ui-appearance={appearance} key={p.person_id} value={p.person_id}>
              {p.legal_name}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance={appearance}>
        Reason{" "}
        <TextInput appearance={appearance}
          required
          minLength={3}
          maxLength={1000}
          value={reason}
          onChange={(event) => setReason(event.target.value)}
        />
      </Field>
      <Button appearance={appearance} clientManaged variant="plain" type="submit" disabled={busy}>
        {busy ? "Saving…" : "Record appointment"}
      </Button>
      {message && <p data-ui-appearance={appearance} role="status">{message}</p>}
    </form></FormDisclosure>
  );
}
function FlowEditor({ appearance = "legacy",
  department,
  policy,
  people,
  positions,
  url,
  onSaved,
}: ({
  department: { id: string; display_name: string };
  policy: Policy | undefined;
  people: Person[];
  positions: Position[];
  url: string;
  onSaved: () => Promise<void>;
}) & { appearance?: UiAppearance }) {
  const original = policy
    ? typeof policy.steps_json === "string"
      ? (JSON.parse(policy.steps_json) as Selector[])
      : policy.steps_json
    : [];
  const [preview,setPreview] = useState<{steps:{name:string}[];blockers:string[];previousVersion:number;newVersion:number;previousSteps:unknown;effect:string;simulations:{requester:string;reviewers:string[];blocker:string|null}[]}|null>(null);
  const [steps, updateSteps] = useState<Selector[]>(original);
  const setSteps:typeof updateSteps = value => {setPreview(null);updateSteps(value);};
  const [reason, setReason] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const options = [
    ...people.map((p) => ({ value: `person:${p.person_id}`, name: `Person · ${p.legal_name}` })),
    ...positions.map((p) => ({ value: `position:${p.id}`, name: `Position · ${p.title}` })),
  ];
  const toValue = (step: Selector) =>
    step.kind === "person" ? `person:${step.personId}` : `position:${step.positionId}`;
  const fromValue = (value: string): Selector =>
    value.startsWith("person:")
      ? { kind: "person", personId: value.slice(7) }
      : { kind: "position", positionId: value.slice(9) };
  return (
    <FormDisclosure appearance="standard"><summary>Edit flow · {department.display_name}</summary>
    <form data-ui-appearance={appearance} onChangeCapture={()=>setPreview(null)}
      className={appearance === "standard" ? "ui-panel" : "card"}
      onSubmit={async (event) => {
        event.preventDefault();
        const invalid = validateFlowSteps(steps);
        if (invalid) {
          setMessage(invalid);
          return;
        }
        setBusy(true);
        setMessage("");
        try {
          if(!preview){const response=await fetch(`${url}/preview`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({departmentId:department.id,expectedVersion:policy?.version??0,steps,reason})});const body=await response.json();if(!response.ok)throw Error(body.error?.message??"Unable to preview this flow.");setPreview(body.data);return;}
          if(preview.blockers.length)return;
          await post(url, {
            departmentId: department.id,
            expectedVersion: policy?.version ?? 0,
            steps,
            reason,
          });
          await onSaved();
          setReason("");
          setMessage("Flow published for new requests.");
        } catch (error) {
          setMessage((error as Error).message);
        } finally {
          setBusy(false);
        }
      }}
    >
      <h3 data-ui-appearance={appearance}>{department.display_name}</h3>
      <FormContext appearance={appearance}
        version={policy?.version ?? 0}
        effect="Publishing applies to new requests. Requests already submitted keep their saved route and decision history."
      />
      {steps.map((step, index) => (
        <div data-ui-appearance={appearance} key={index} style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 8 }}>
          <span data-ui-appearance={appearance}>{index + 1}.</span>
          <Combobox label={`Approver ${index+1}`} options={options.map(o=>({value:o.value,label:o.name}))} value={toValue(step)} onValueChange={value=>{setPreview(null);setSteps(values=>values.map((s,i)=>i===index?fromValue(value):s));}} disabled={busy}/>

          <Button appearance={appearance}
            variant="plain"
            type="button"
            disabled={index === 0}
            onClick={() =>
              setSteps((values) => {
                const next = [...values];
                [next[index - 1], next[index]] = [next[index], next[index - 1]];
                return next;
              })
            }
          >
            ↑
          </Button>
          <Button appearance={appearance}
            variant="plain"
            type="button"
            disabled={index === steps.length - 1}
            onClick={() =>
              setSteps((values) => {
                const next = [...values];
                [next[index], next[index + 1]] = [next[index + 1], next[index]];
                return next;
              })
            }
          >
            ↓
          </Button>
          <Button appearance={appearance}
            variant="plain"
            type="button"
            onClick={() => setSteps((values) => values.filter((_, i) => i !== index))}
          >
            Remove
          </Button>
        </div>
      ))}
      <Button appearance={appearance}
        variant="plain"
        type="button"
        disabled={steps.length >= 20 || !options.length}
        onClick={() => setSteps((values) => [...values, fromValue(options[0].value)])}
      >
        Add step
      </Button>
      <Field appearance={appearance}>
        Reason{" "}
        <TextInput appearance={appearance}
          required
          minLength={3}
          maxLength={1000}
          value={reason}
          onChange={(event) => setReason(event.target.value)}
        />
      </Field>
      {preview&&<section className="request-route"><h3>Review policy version {preview.previousVersion} → {preview.newVersion}</h3><ol>{preview.steps.map((s,i)=><li key={i}>{s.name}</li>)}</ol><details><summary>Previous recorded selectors</summary><pre>{JSON.stringify(preview.previousSteps,null,2)}</pre></details><p>{preview.effect}</p><details><summary>Requester examples & independence checks</summary>{preview.simulations.map((s,i)=><p key={i}>{s.requester} → {s.blocker??s.reviewers.join(" → ")}</p>)}</details>{preview.blockers.map(b=><p key={b} role="alert">{b}</p>)}</section>}
      <Button appearance={appearance} clientManaged variant="plain" type="submit" disabled={busy || !steps.length || !!preview?.blockers.length}>
        {busy ? "Working…" : preview ? "Confirm & publish flow" : "Review route & changes"}
      </Button>
      {message && <p data-ui-appearance={appearance} role="status">{message}</p>}
    </form></FormDisclosure>
  );
}
export function DepartmentAuthorityWorkspace({ appearance = "legacy", holdingSlug, initial, people, positions }: (Props) & { appearance?: UiAppearance }) {
  const [data, setData] = useState(initial);
  const root = `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/authority`;
  const refresh = async () => {
    const response = await fetch(root, { cache: "no-store" });
    if (!response.ok) throw new Error("Could not reload authority data.");
    setData((await response.json()).data as Data);
  };
  const seats: Seat[] = [
    ...data.seats,
    ...data.departments
      .filter((d) => !data.seats.some((s) => s.kind === "head" && s.scope_id === d.id))
      .map((d) => ({
        kind: "head" as const,
        scope_id: d.id,
        version: 0,
        person_id: null,
        position_id: null,
        person_name: null,
        department_name: d.display_name,
      })),
  ];
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-stack" : "content-stack"} aria-label="Appointments and department approval flows">
      <div data-ui-appearance={appearance}>
        <h2 data-ui-appearance={appearance}>Appointments</h2>
        <p data-ui-appearance={appearance} className="muted">
          Appointment authority applies immediately. Directory work has a separate delivery status.
        </p>
      </div>
      {data.canAppointKinds.length > 0 && (
        <div data-ui-appearance={appearance} className="grid-two">
          {seats
            .filter((seat) => data.canAppointKinds.includes(seat.kind))
            .map((seat) => (
              <AppointmentEditor appearance={appearance}
                key={`${seat.kind}:${seat.scope_id}:${seat.version}`}
                seat={seat}
                people={people}
                positions={positions}
                url={`${root}/appointments`}
                onSaved={refresh}
              />
            ))}
        </div>
      )}
      <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
        {seats
          .filter((seat) => !data.canAppointKinds.includes(seat.kind))
          .map((s) => (
            <p data-ui-appearance={appearance} key={`${s.kind}:${s.scope_id}`}>
              {label[s.kind]}
              {s.department_name ? ` · ${s.department_name}` : ""}: {s.person_name ?? "Vacant"}
            </p>
          ))}
      </div>
      <div data-ui-appearance={appearance}>
        <h2 data-ui-appearance={appearance}>Department approval flows</h2>
        <p data-ui-appearance={appearance} className="muted">
          Choose people or positions in order. A position must have one active holder when a request
          is sent.
        </p>
      </div>
      {data.departments.filter(d=>!data.canPublishDepartmentIds.includes(d.id)).map(d=><section className="ui-panel" key={d.id}><h3>{d.display_name}</h3><p>{data.policies.find(p=>p.department_id===d.id)?`Policy version ${data.policies.find(p=>p.department_id===d.id)!.version}`:"No published flow"}</p><p>Read only: the appointed department head, or CEO while the head seat is vacant, can publish. Appointment authority does not grant publishing authority.</p></section>)}
      {data.canPublishDepartmentIds.map((departmentId) => {
        const department = data.departments.find((d) => d.id === departmentId)!;
        const policy = data.policies.find((p) => p.department_id === departmentId);
        return (
          <FlowEditor appearance={appearance}
            key={`${departmentId}:${policy?.version ?? 0}`}
            department={department}
            policy={policy}
            people={people}
            positions={positions}
            url={`${root}/flows`}
            onSaved={refresh}
          />
        );
      })}
      <details data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
        <summary data-ui-appearance={appearance}>Appointment history</summary>
        {data.history.map((item) => (
          <p data-ui-appearance={appearance} key={item.id}>
            {label[item.kind as keyof typeof label]} · v{item.version} ·{" "}
            {item.provenance === "legacy"
              ? "Imported legacy record"
              : "Appointed by " + item.appointed_by}{" "}
            · {item.reason}
          </p>
        ))}
      </details>
    </section>
  );
}
