"use client";

import { useState } from "react";
import type { WorkspaceCompany } from "@/server/authorization";

export function CompanySwitcher({
  workspaceBase,
  activeCompanyId,
  companies,
}: {
  workspaceBase: string;
  activeCompanyId: string;
  companies: WorkspaceCompany[];
}) {
  const [pending, setPending] = useState(false);

  return (
    <label className="company-switcher">
      <span className="sr-only">Active company</span>
      <select
        aria-label="Active company"
        value={activeCompanyId}
        disabled={pending}
        onChange={async (event) => {
          setPending(true);
          const response = await fetch(
            `/api/v1${workspaceBase}/companies/active`,
            {
              method: "POST",
              headers: { "content-type": "application/json" },
              body: JSON.stringify({ companyId: event.target.value }),
            },
          );
          if (response.ok) window.location.reload();
          else setPending(false);
        }}
      >
        {companies.map((company) => (
          <option key={company.id} value={company.id}>
            {company.name}
          </option>
        ))}
      </select>
    </label>
  );
}
