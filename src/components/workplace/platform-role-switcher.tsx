"use client";

import { workFetch } from "@/lib/work-fetch";

import { useState } from "react";
import type { WorkAssignment } from "@/server/authorization";
import { useContextSwitch } from "./context-switch-provider";

export function PlatformRoleSwitcher({
  activeAssignmentId,
  assignments,
}: {
  activeAssignmentId: string;
  assignments: WorkAssignment[];
}) {
  const { switchContext, busy } = useContextSwitch();
  const [error, setError] = useState("");
  return (
    <div>
    <label className="company-switcher">
      <span className="sr-only">Platform role</span>
      <select
        aria-label="Platform role"
        value={activeAssignmentId}
        disabled={busy}
        onChange={async (event) => {
          const assignmentId = event.target.value;
          if (assignmentId === activeAssignmentId || busy) return;
          setError("");
          try {
            await switchContext({ companyId: document.querySelector<HTMLElement>("[data-work-company]")?.dataset.workCompany, action: async () => {
              const response = await workFetch("/api/v1/platform/assignments/active", {
                method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ assignmentId }),
              });
              const body = await response.json();
              if (!response.ok) throw new Error(body.error?.message ?? "Platform role could not be selected.");
            } });
          } catch (cause) { setError(cause instanceof Error ? cause.message : "Platform role could not be selected."); }
        }}
      >
        {assignments.map((assignment) => (
          <option key={assignment.id} value={assignment.id}>
            {assignment.role.replaceAll("_", " ")}
          </option>
        ))}
      </select>
    </label>
    {error && <p role="alert">{error}</p>}
    </div>
  );
}
