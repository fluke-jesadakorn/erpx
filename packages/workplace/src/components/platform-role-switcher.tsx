"use client";
import { InlineNotice } from "@erpx/ui/components/patterns";

import { Field } from "@erpx/ui/components/field";
import { Select } from "@erpx/ui/components/input";

import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";

import { useState } from "react";
import type {PlatformRoleDisplayChoice} from "@erpx/feature-identity_access/lib/work-role-display";
import { useContextSwitch } from "@erpx/workplace/components/context-switch-context";

export function PlatformRoleSwitcher({
  activeAssignmentId,
  assignments,
}: {
  activeAssignmentId: string;
  assignments: readonly PlatformRoleDisplayChoice[];
}) {
  const { switchContext, busy } = useContextSwitch();
  const [error, setError] = useState("");
  return (
    <div>
      <Field appearance="standard" className="company-switcher">
        <span className="sr-only">Platform role</span>
        <Select appearance="standard"
          aria-label="Platform role"
          value={activeAssignmentId}
          disabled={busy}
          onChange={async (event) => {
            const assignmentId = event.target.value;
            if (assignmentId === activeAssignmentId || busy) return;
            setError("");
            try {
              await switchContext({
                companyId:
                  document.querySelector<HTMLElement>("[data-work-company]")?.dataset.workCompany,
                action: async () => {
                  const response = await workFetch("/api/v1/platform/assignments/active", {
                    method: "POST",
                    headers: { "content-type": "application/json" },
                    body: JSON.stringify({ assignmentId }),
                  });
                  const body = await response.json();
                  if (!response.ok)
                    throw new Error(body.error?.message ?? "Platform role could not be selected.");
                },
              });
            } catch (cause) {
              setError(
                cause instanceof Error ? cause.message : "Platform role could not be selected.",
              );
            }
          }}
        >
          {assignments.map((assignment) => (
            <option key={assignment.id} value={assignment.id}>
              {assignment.role.replaceAll("_", " ")}
            </option>
          ))}
        </Select>
      </Field>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    </div>
  );
}
