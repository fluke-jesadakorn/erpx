"use client";

import { workFetch } from "@/lib/work-fetch";

import { useHydrated } from "@/components/ui/use-hydrated";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import type { OrganizationOverview } from "@/lib/organization";
type Row = Record<string, unknown>;
export function PeopleEditor({
  base,
  person,
  nodes,
  levels = [],
  people = [],
  levelManagedSeparately = false,
  member,
  returnTo,
  organization,
  initialPositionId,
  lifecycleManaged = false,
  initialOpen = false,
  alwaysOpen = false,
}: {
  base: string;
  person?: Row;
  nodes: Row[];
  levels?: Row[];
  people?: Row[];
  levelManagedSeparately?: boolean;
  member?: { id: string; name: string; email: string };
  returnTo?: string;
  organization?: OrganizationOverview;
  initialPositionId?:string;
  lifecycleManaged?: boolean;
  initialOpen?: boolean;
  alwaysOpen?: boolean;
}) {
  const ready = useHydrated();
  const router = useRouter();
  const [open, setOpen] = useState(initialOpen || !!initialPositionId);
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const [branch, setBranch] = useState(
    String(
      person?.branch_id ?? organization?.positions.find(p=>p.id===initialPositionId)?.branch_id ?? nodes.find((n) => n.kind === "branch")?.id ?? "",
    ),
  );
  const command = useRef<{ payload: string; key: string } | null>(null);
  const [positionId,setPositionId]=useState(String(person?.position_id??initialPositionId??""));
  const [levelId,setLevelId]=useState(String(person?.level_id??organization?.positions.find(p=>p.id===initialPositionId)?.level_id??""));
  const selectedPosition=organization?.positions.find(p=>p.id===positionId);
  const selectedLevel=organization?.levels.find(l=>l.id===levelId)?.level_number;
  const managerPeople=[...new Map([...people,...(organization?.people??[])].map(p=>[String(p.person_id),p])).values()];
  async function send(path: string, input: unknown) {
    const payload = JSON.stringify(input);
    if (command.current?.payload !== payload)
      command.current = { payload, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const response = await workFetch(
        `${base.replace("/holdings/", "/api/v1/holdings/")}/people/directory${path}`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": command.current.key,
          },
          body: payload,
        },
      );
      const result = (await response.json()) as {
        data: { id: string };
        error?: { message: string };
      };
      if (!response.ok)
        throw new Error(result.error?.message ?? "Unable to save.");
      command.current = null;
      setOpen(false);
      router.push(returnTo ?? `${base}/people/${result.data.id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save.");
    } finally {
      setPending(false);
    }
  }
  return (
    <section className="card">
      <div className="section-head">
        <h2>{person ? "Manage employee" : "Add a person"}</h2>
        {!alwaysOpen && (
          <button
            className="button"
            disabled={!ready}
            onClick={() => setOpen(!open)}
          >
            {open ? "Close form" : person ? "Edit employee" : "Add person"}
          </button>
        )}
      </div>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {(open || alwaysOpen) && (
        <form
          className="business-form"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(person ? `/${person.id}` : "", {
              legalName: f.get("legalName"),
              preferredName: f.get("preferredName"),
              email: f.get("email"),
              phone: f.get("phone"),
              countryCode: f.get("countryCode"),
              branchId: branch,
              departmentId: f.get("departmentId") || null,
              jobTitle: f.get("jobTitle"),
              workerType:
                f.get("employmentType") === "contractor"
                  ? "contractor"
                  : "employee",
              employmentType: f.get("employmentType"),
              startDate: f.get("startDate"),
              currency: f.get("currency"),
              managerPersonId: f.get("managerPersonId") || null,
              levelId: f.get("levelId") || null,
              positionId: positionId || null,
              reason: f.get("reason") || "Staff profile updated",
              ...(member ? { linkedMemberUserId: member.id } : {}),
              ...(person ? { expectedVersion: person.version } : {}),
            });
          }}
        >
          <label>
            Legal name
            <input
              name="legalName"
              required
              minLength={2}
              maxLength={200}
              defaultValue={String(person?.legal_name ?? member?.name ?? "")}
            />
          </label>
          <label>
            Preferred name
            <input
              name="preferredName"
              maxLength={160}
              defaultValue={String(person?.preferred_name ?? "")}
            />
          </label>
          <label>
            Work email
            <input
              name="email"
              type="email"
              required
              defaultValue={String(person?.email ?? member?.email ?? "")}
              readOnly={!!member}
            />
          </label>
          <label>
            Phone
            <input
              name="phone"
              type="tel"
              defaultValue={String(person?.phone ?? "")}
            />
          </label>
          <label>
            Country code
            <input
              name="countryCode"
              required
              pattern="[A-Z]{2}"
              maxLength={2}
              defaultValue={String(person?.country_code ?? "")}
            />
          </label>
          <label>
            Branch
            <select
              name="branchId"
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              required
            >
              {nodes
                .filter((n) => n.kind === "branch")
                .map((n) => (
                  <option key={String(n.id)} value={String(n.id)}>
                    {String(n.display_name)}
                  </option>
                ))}
            </select>
          </label>
          <label>
            Department
            <select
              name="departmentId"
              key={branch}
              defaultValue={String(person?.department_id ?? selectedPosition?.department_id ?? "")}
            >
              <option value="">No department</option>
              {nodes
                .filter(
                  (n) => n.kind === "department" && n.branch_id === branch,
                )
                .map((n) => (
                  <option key={String(n.id)} value={String(n.id)}>
                    {String(n.display_name)}
                  </option>
                ))}
            </select>
          </label>
          {organization&&<label>Position<select value={positionId} onChange={e=>{setPositionId(e.target.value);const p=organization.positions.find(p=>p.id===e.target.value);if(p)setLevelId(p.level_id);}}><option value="">Not assigned — onboarding draft</option>{organization.positions.filter(p=>p.active===1&&(!p.branch_id||p.branch_id===branch)).map(p=><option key={p.id} value={p.id}>{p.title} · Level {p.level_number}</option>)}</select><small>Positions supply a default level. Confirm the staff level and actual manager below.</small></label>}
          {levelManagedSeparately && !organization ? (
            <input
              type="hidden"
              name="levelId"
              value={String(person?.level_id ?? "")}
            />
          ) : (
            <label>
              Staff level
              <select
                name="levelId"
                value={levelId}
                onChange={e=>setLevelId(e.target.value)}
                required={person?.engagement_status === "active"}
              >
                <option value="">Not set</option>
                {levels
                  .filter((l) => (l.active === 1 && l.level_number != null) || l.id === person?.level_id)
                  .map((l) => (
                    <option key={String(l.id)} value={String(l.id)}>
                      {String(l.name)} · {l.level_number?`Level ${l.level_number}`:"Legacy level — map to numbered catalog"}
                    </option>
                  ))}
              </select>
            </label>
          )}
          <label>
            Reporting manager
            <select
              name="managerPersonId"
              key={`${positionId}-${levelId}`}
              defaultValue={selectedPosition?.is_ceo?"":String(person?.manager_person_id ?? "")}
              disabled={Boolean(selectedPosition?.is_ceo)}
            >
              <option value="">{selectedPosition?.is_ceo?"CEO — no reporting manager":"Choose a manager before activation"}</option>
              {managerPeople
                .filter((p) => p.person_id !== person?.id && (!selectedLevel || (p.level_number != null && Number(p.level_number)<selectedLevel)) && (!p.status || p.status === "active"))
                .map((p) => (
                  <option
                    key={String(p.engagement_id)}
                    value={String(p.person_id)}
                  >
                    {String(p.legal_name)}{p.level_number?` · Level ${p.level_number}`:""}
                  </option>
                ))}
            </select>
          </label>
          <label>
            Job title
            <input
              name="jobTitle"
              key={positionId}
              required
              minLength={2}
              defaultValue={selectedPosition?.title??String(person?.job_title ?? selectedPosition?.title ?? "")}
            />
          </label>
          <label>
            Employment type
            <select
              name="employmentType"
              defaultValue={String(person?.employment_type ?? "salaried")}
            >
              <option value="salaried">Salaried employee</option>
              <option value="hourly">Hourly employee</option>
              <option value="shift">Shift employee</option>
              <option value="contractor">Contractor</option>
            </select>
          </label>
          <label>
            Start date
            <input
              name="startDate"
              type="date"
              required
              defaultValue={String(
                person?.start_date ?? new Date().toISOString().slice(0, 10),
              )}
            />
          </label>
          <label>
            Pay currency
            <select
              name="currency"
              defaultValue={String(person?.currency ?? "USD")}
            >
              {["THB", "USD", "EUR", "GBP", "SGD", "CAD"].map((c) => (
                <option key={c}>{c}</option>
              ))}
            </select>
          </label>
          <label>Change reason<input name="reason" minLength={3} maxLength={1000} defaultValue={person?"":"Initial onboarding"} placeholder="Describe a transfer, promotion or reporting change" required={!!person}/></label>
          <div className="form-actions">
            <button
              className="button"
              type="submit"
              disabled={pending || !ready}
            >
              {pending ? "Saving…" : "Save employee"}
            </button>
          </div>
        </form>
      )}
      {person && (
        <div className="form-actions">
          {(!lifecycleManaged || person.status === "archived") && <button
            className="button secondary"
            disabled={pending || !ready}
            onClick={() =>
              void send(`/${person.id}/status`, {
                status:
                  person.status === "active" &&
                  person.engagement_status === "active"
                    ? "inactive"
                    : "active",
                expectedVersion: person.version,
              })
            }
          >
            {person.status === "active" && person.engagement_status === "active"
              ? "Deactivate employee"
              : "Activate employee"}
          </button>}
          <button
            className="button secondary"
            disabled={pending || !ready || person.status === "archived" || (lifecycleManaged && person.engagement_status !== "ended")}
            onClick={() =>
              void send(`/${person.id}/status`, {
                status: "archived",
                expectedVersion: person.version,
              })
            }
          >
            Archive employee
          </button>
          <p className="muted">
            {lifecycleManaged ? "Use the lifecycle panel to suspend access or end employment. Archive retains history after employment ends." : "Deactivation revokes this company’s role assignments. Archived profiles retain their history and can be restored."}
          </p>
        </div>
      )}
    </section>
  );
}
