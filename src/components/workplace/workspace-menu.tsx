"use client";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useState } from "react";
import Link from "next/link";
import { Building2, ChevronDown } from "lucide-react";
import type {
  AuthorizedWorkspace,
  WorkAssignment,
  WorkspaceCompany,
} from "@/server/authorization";
import { CompanySwitcher } from "./company-switcher";
import { Dialog } from "@/components/ui/dialog";
import { businessRoleLabel } from "@/lib/business-roles";
import styles from "./shell.module.css";
export function WorkspaceMenu({
  base,
  label,
  companyId,
  companies,
  assignments,
  workspaces,
}: {
  base: string;
  label: string;
  companyId: string;
  companies: WorkspaceCompany[];
  assignments: WorkAssignment[];
  activeAssignmentId: string;
  workspaces: AuthorizedWorkspace[];
}) {
  const ready = useHydrated();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        className={styles.workspaceTrigger}
        type="button"
        disabled={!ready}
        aria-label="Workspace menu"
        aria-haspopup="dialog"
        title={`Switch workspace: ${label}`}
        aria-expanded={open}
        onClick={() => setOpen(true)}
      >
        <Building2 size={18} aria-hidden="true" />
        <ChevronDown size={12} aria-hidden="true" />
      </button>
      <Dialog title="Workspace" open={open} onClose={() => setOpen(false)}>
        {companies.length > 1 && (
          <section className={styles.workspaceSection}>
            <h3>Company</h3>
            <CompanySwitcher
              workspaceBase={base}
              activeCompanyId={companyId}
              companies={companies}
            />
          </section>
        )}
        <section className={styles.workspaceSection}>
          <h3>Authorized workspaces</h3>
          <div className={styles.menuLinks}>
            {workspaces.map((w) => (
              <a key={w.key} href={w.href} onClick={() => setOpen(false)}>
                {w.label}
              </a>
            ))}
          </div>
        </section>
        <section className={styles.workspaceSection}>
          <h3>Your responsibilities</h3>
          <p>
            Work combines your authorized responsibilities in this workspace.
          </p>
          <div className={styles.menuLinks}>
            <Link href={`${base}/work`} onClick={() => setOpen(false)}>
              All responsibilities
            </Link>
            <Link
              href={`${base}/work?responsibility=requested`}
              onClick={() => setOpen(false)}
            >
              My requests
            </Link>
          </div>
          {[...new Set(assignments.map((a) => a.role))].map((role) => (
            <p key={role}>{businessRoleLabel(role)}</p>
          ))}
        </section>
      </Dialog>
    </>
  );
}
