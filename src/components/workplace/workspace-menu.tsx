"use client";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useState } from "react";
import { Building2, ChevronDown } from "lucide-react";
import type {
  WorkspaceCompany,
} from "@/server/authorization";
import { CompanySwitcher } from "./company-switcher";
import { Dialog } from "@/components/ui/dialog";
import type { WorkspaceChoice } from "@/lib/workspace-choice";
import styles from "./shell.module.css";
export function WorkspaceMenu({
  base,
  label,
  companyId,
  companies = [],
  choices,
}: {
  base: string;
  label: string;
  companyId?: string;
  companies?: WorkspaceCompany[];
  choices: WorkspaceChoice[];
}) {
  const ready = useHydrated();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        className={styles.workspaceTrigger}
        type="button"
        disabled={!ready}
        aria-label="Switch workspace"
        aria-haspopup="dialog"
        title={`Switch workspace: ${label}`}
        aria-expanded={open}
        onClick={() => setOpen(true)}
      >
        <Building2 size={18} aria-hidden="true" />
        <span>Switch workspace</span>
        <ChevronDown size={12} aria-hidden="true" />
      </button>
      <Dialog title="Switch workspace" open={open} onClose={() => setOpen(false)}>
        {companyId && companies.length > 1 && (
          <section className={styles.workspaceSection}>
            <h3>Company</h3>
            <CompanySwitcher workspaceBase={base} activeCompanyId={companyId} companies={companies} />
          </section>
        )}
        {(["business", "platform"] as const).filter(kind => kind === "business" || choices.some(choice => choice.kind === kind)).map(kind => (
          <section key={kind} className={styles.workspaceSection}>
            <h3>{kind === "business" ? "Business workspaces" : "Platform administration"}</h3>
            {kind === "business" && !choices.some(choice => choice.kind === kind) && <p>No business workspaces assigned.</p>}
            <div className={styles.menuLinks}>
              {choices.filter(choice => choice.kind === kind).map(choice => {
                const current = kind === "platform" ? base === "/platform" : base.split("/")[2] === choice.destination.split("/")[2];
                return <a key={choice.destination} href={choice.destination} aria-current={current ? "location" : undefined}>
                  {choice.label}{current ? " · Current area" : ""}
                </a>;
              })}
            </div>
          </section>
        ))}
      </Dialog>
    </>
  );
}
