"use client";

import { workFetch } from "@/lib/work-fetch";

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
  const [error, setError] = useState("");

  return (
    <label className="company-switcher">
      <span className="sr-only">Active company</span>
      <select
        aria-label="Active company"
        value={activeCompanyId}
        disabled={pending}
        onChange={async (event) => {
          setPending(true);
          setError("");
          try {
          const response = await workFetch(
            `/api/v1${workspaceBase}/companies/active`,
            {
              method: "POST",
              headers: { "content-type": "application/json" },
              body: JSON.stringify({ companyId: event.target.value }),
            },
          );
          if (response.ok) window.location.reload();
          else {
            setError("Company could not be selected. Check your access and try again.");
            setPending(false);
          }
          } catch {
            setError("Company could not be selected. Check your connection and try again.");
            setPending(false);
          }
        }}
      >
        {companies.map((company) => (
          <option key={company.id} value={company.id}>
            {company.name}
          </option>
        ))}
      </select>
      {error && <span role="alert">{error}</span>}
    </label>
  );
}
