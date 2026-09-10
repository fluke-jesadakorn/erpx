"use client";

import { useState } from "react";
import type { WorkAssignment } from "@/server/authorization";

export function PlatformRoleSwitcher({
  activeAssignmentId,
  assignments,
}: {
  activeAssignmentId: string;
  assignments: WorkAssignment[];
}) {
  const [pending, setPending] = useState(false);
  return (
    <label className="company-switcher">
      <span className="sr-only">Platform role</span>
      <select
        aria-label="Platform role"
        value={activeAssignmentId}
        disabled={pending}
        onChange={async (event) => {
          setPending(true);
          const response = await fetch("/api/v1/platform/assignments/active", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ assignmentId: event.target.value }),
          });
          if (response.ok) window.location.reload();
          else setPending(false);
        }}
      >
        {assignments.map((assignment) => (
          <option key={assignment.id} value={assignment.id}>
            {assignment.role.replaceAll("_", " ")}
          </option>
        ))}
      </select>
    </label>
  );
}
