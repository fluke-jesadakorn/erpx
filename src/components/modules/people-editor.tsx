"use client";

import { useHydrated } from "@/components/ui/use-hydrated";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
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
}: {
  base: string;
  person?: Row;
  nodes: Row[];
  levels?: Row[];
  people?: Row[];
  levelManagedSeparately?: boolean;
  member?: { id: string; name: string; email: string };
  returnTo?: string;
}) {
  const ready = useHydrated();
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const [branch, setBranch] = useState(
    String(
      person?.branch_id ?? nodes.find((n) => n.kind === "branch")?.id ?? "",
    ),
  );
  const command = useRef<{ payload: string; key: string } | null>(null);
  async function send(path: string, input: unknown) {
    const payload = JSON.stringify(input);
    if (command.current?.payload !== payload)
      command.current = { payload, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const response = await fetch(
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
        <button
          className="button"
          disabled={!ready}
          onClick={() => setOpen(!open)}
        >
          {open ? "Close form" : person ? "Edit employee" : "Add person"}
        </button>
      </div>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {open && (
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
              defaultValue={String(person?.department_id ?? "")}
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
          {levelManagedSeparately ? (
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
                defaultValue={String(person?.level_id ?? "")}
                required={person?.engagement_status === "active"}
              >
                <option value="">Not set</option>
                {levels
                  .filter((l) => l.active === 1 || l.id === person?.level_id)
                  .map((l) => (
                    <option key={String(l.id)} value={String(l.id)}>
                      {String(l.name)} · rank {String(l.rank)}
                    </option>
                  ))}
              </select>
            </label>
          )}
          <label>
            Reporting manager
            <select
              name="managerPersonId"
              defaultValue={String(person?.manager_person_id ?? "")}
            >
              <option value="">No reporting manager</option>
              {people
                .filter((p) => p.person_id !== person?.id)
                .map((p) => (
                  <option
                    key={String(p.engagement_id)}
                    value={String(p.person_id)}
                  >
                    {String(p.legal_name)}
                  </option>
                ))}
            </select>
          </label>
          <label>
            Job title
            <input
              name="jobTitle"
              required
              minLength={2}
              defaultValue={String(person?.job_title ?? "")}
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
          <button
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
          </button>
          <button
            className="button secondary"
            disabled={pending || !ready || person.status === "archived"}
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
            Deactivation revokes this company’s role assignments. Archived
            profiles retain their history and can be restored.
          </p>
        </div>
      )}
    </section>
  );
}
