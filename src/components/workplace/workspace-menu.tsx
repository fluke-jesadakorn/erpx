"use client";
/* eslint-disable @next/next/no-img-element -- Private workspace logos are served through authenticated attachment routes. */

import { useHydrated } from "@/components/ui/use-hydrated";
import { useState } from "react";
import {
  Building2,
  ChevronDown,
  Network,
  ShieldCheck,
} from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work-fetch";
import { waitForWorkspaceMemory } from "@/lib/workspace-memory-client";
import type {
  CurrentWorkspaceChoice,
  WorkspaceBranchChoice,
  WorkspaceChoice,
  WorkspaceCompanyChoice,
  WorkspaceDepartmentChoice,
  WorkspaceChoiceBrand,
} from "@/lib/workspace-choice";
import styles from "./shell.module.css";

function CurrentArea() {
  return <span className={styles.workspaceCurrent}>Current area</span>;
}

function ChoiceBrand({ brand }: { brand?: WorkspaceChoiceBrand }) {
  if (!brand) return <Building2 size={17} aria-hidden="true" />;
  return <span className={styles.choiceBrand} aria-hidden="true">{brand.primaryUrl ? <img src={brand.primaryUrl} alt="" /> : brand.initials}</span>;
}

export function WorkspaceMenu({
  label,
  choices,
  current,
}: {
  label: string;
  choices: WorkspaceChoice[];
  current: CurrentWorkspaceChoice;
}) {
  const ready = useHydrated();
  const [open, setOpen] = useState(false);
  const [pendingCompany, setPendingCompany] = useState("");
  const [error, setError] = useState("");
  const holdings = choices.filter((choice) => choice.kind === "holding");
  const platform = choices.find((choice) => choice.kind === "platform");

  const companyIsCurrent = (
    holdingSlug: string,
    company: WorkspaceCompanyChoice,
  ) =>
    current.kind === "holding" &&
    current.holdingSlug === holdingSlug &&
    current.companyId === company.id;
  const branchIsCurrent = (
    holdingSlug: string,
    companyId: string,
    branch: WorkspaceBranchChoice,
  ) =>
    current.kind === "branch" &&
    current.holdingSlug === holdingSlug &&
    current.companyId === companyId &&
    current.branchId === branch.id;
  const departmentIsCurrent = (
    holdingSlug: string,
    companyId: string,
    department: WorkspaceDepartmentChoice,
  ) =>
    current.kind === "department" &&
    current.holdingSlug === holdingSlug &&
    current.companyId === companyId &&
    current.departmentId === department.id;

  async function selectCompany(
    holdingSlug: string,
    company: WorkspaceCompanyChoice,
  ) {
    if (!company.destination || !company.switchEndpoint) return;
    if (companyIsCurrent(holdingSlug, company)) {
      setOpen(false);
      return;
    }
    setPendingCompany(`${holdingSlug}:${company.id}`);
    setError("");
    try {
      await waitForWorkspaceMemory();
      const response = await workFetch(company.switchEndpoint, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ companyId: company.id }),
      });
      if (!response.ok) {
        setError(
          "Business could not be selected. Check your access and try again.",
        );
        setPendingCompany("");
        return;
      }
      window.location.assign(company.destination);
    } catch {
      setError(
        "Business could not be selected. Check your connection and try again.",
      );
      setPendingCompany("");
    }
  }

  function departmentLink(
    holdingSlug: string,
    companyId: string,
    department: WorkspaceDepartmentChoice,
  ) {
    const isCurrent = departmentIsCurrent(
      holdingSlug,
      companyId,
      department,
    );
    return (
      <a
        key={department.id}
        className={`${styles.workspaceChoice} ${styles.workspaceDepartmentChoice}`}
        href={department.destination}
        aria-current={isCurrent ? "location" : undefined}
        onClick={() => setOpen(false)}
      >
        <Network size={16} aria-hidden="true" />
        <span>{department.label}</span>
        {isCurrent && <CurrentArea />}
      </a>
    );
  }

  return (
    <>
      <button
        className={styles.workspaceTrigger}
        type="button"
        disabled={!ready}
        aria-label="Switch workspace"
        aria-haspopup="dialog"
        title={`Change workspace (currently ${label})`}
        aria-expanded={open}
        onClick={() => {
          setError("");
          setOpen(true);
        }}
      >
        <Building2 size={18} aria-hidden="true" />
        <span className={styles.workspaceTriggerLabel}>{label}</span>
        <ChevronDown
          className={styles.workspaceTriggerChevron}
          size={14}
          aria-hidden="true"
        />
      </button>
      <Dialog title="Switch workspace" open={open} onClose={() => setOpen(false)}>
        <section className={styles.workspaceSection}>
          <h3>Business workspaces</h3>
          {!holdings.length && <p>No business workspaces assigned.</p>}
          <div className={styles.workspaceTree}>
            {holdings.map((holding) => (
              <section
                key={holding.holdingSlug}
                className={styles.workspaceHolding}
              >
                <h4><ChoiceBrand brand={holding.brand} />{holding.label}</h4>
                {holding.companies.map((company) => {
                  const companyCurrent = companyIsCurrent(
                    holding.holdingSlug,
                    company,
                  );
                  const pendingKey = `${holding.holdingSlug}:${company.id}`;
                  return (
                    <div key={company.id} className={styles.workspaceCompany}>
                      {company.destination && company.switchEndpoint ? (
                        <button
                          type="button"
                          className={`${styles.workspaceChoice} ${styles.workspaceCompanyChoice}`}
                          aria-current={companyCurrent ? "location" : undefined}
                          aria-busy={pendingCompany === pendingKey}
                          disabled={!!pendingCompany}
                          onClick={() => selectCompany(holding.holdingSlug, company)}
                        >
                          <Building2 size={17} aria-hidden="true" />
                          <span>{company.label}</span>
                          {companyCurrent && <CurrentArea />}
                        </button>
                      ) : (
                        <div
                          className={`${styles.workspaceChoice} ${styles.workspaceCompanyLabel}`}
                        >
                          <Building2 size={17} aria-hidden="true" />
                          <span>{company.label}</span>
                        </div>
                      )}
                      {(company.branches.length > 0 ||
                        company.departments.length > 0) && (
                        <div className={styles.workspaceLocations}>
                          {company.branches.map((branch) => {
                            const branchCurrent = branchIsCurrent(
                              holding.holdingSlug,
                              company.id,
                              branch,
                            );
                            return (
                              <div key={branch.id}>
                                {branch.destination ? (
                                  <a
                                    className={styles.workspaceChoice}
                                    href={branch.destination}
                                    aria-current={
                                      branchCurrent ? "location" : undefined
                                    }
                                    onClick={() => setOpen(false)}
                                  >
                                    <ChoiceBrand brand={branch.brand} />
                                    <span>{branch.label}</span>
                                    {branchCurrent && <CurrentArea />}
                                  </a>
                                ) : (
                                  <div
                                    className={`${styles.workspaceChoice} ${styles.workspaceBranchLabel}`}
                                  >
                                    <ChoiceBrand brand={branch.brand} />
                                    <span>{branch.label}</span>
                                  </div>
                                )}
                                {branch.departments.length > 0 && (
                                  <div className={styles.workspaceDepartments}>
                                    {branch.departments.map((department) =>
                                      departmentLink(
                                        holding.holdingSlug,
                                        company.id,
                                        department,
                                      ),
                                    )}
                                  </div>
                                )}
                              </div>
                            );
                          })}
                          {company.departments.map((department) =>
                            departmentLink(
                              holding.holdingSlug,
                              company.id,
                              department,
                            ),
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
              </section>
            ))}
          </div>
          {error && (
            <p className={styles.workspaceError} role="alert">
              {error}
            </p>
          )}
        </section>
        {platform && (
          <section className={styles.workspaceSection}>
            <h3>Platform administration</h3>
            <div className={styles.workspaceTree}>
              <a
                className={styles.workspaceChoice}
                href={platform.destination}
                aria-current={
                  current.kind === "platform" ? "location" : undefined
                }
                onClick={() => setOpen(false)}
              >
                <ShieldCheck size={17} aria-hidden="true" />
                <span>{platform.label}</span>
                {current.kind === "platform" && <CurrentArea />}
              </a>
            </div>
          </section>
        )}
      </Dialog>
    </>
  );
}
