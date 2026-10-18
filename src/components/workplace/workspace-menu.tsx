"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { Button } from "@/components/ui/foundation";

/* eslint-disable @next/next/no-img-element -- Private workspace logos are served through authenticated attachment routes. */

import { useHydrated } from "@/components/ui/use-hydrated";
import { useRef, useState, type MouseEvent } from "react";
import Link from "@/components/workplace/workspace-link";
import { usePathname } from "next/navigation";
import { Building2, ChevronDown, Network, ShieldCheck } from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { waitForWorkspaceMemory } from "@/lib/work_projection/workspace-memory-client";
import { leavePushForWorkspaceChange } from "@/lib/shared/push-client";
import type {
  CurrentWorkspaceChoice,
  WorkspaceBranchChoice,
  WorkspaceChoice,
  WorkspaceCompanyChoice,
  WorkspaceDepartmentChoice,
  WorkspaceChoiceBrand,
} from "@/lib/work_projection/workspace-choice";
import styles from "./shell.module.css";
import { type UiAppearance } from "@/components/ui/foundation";

function CurrentArea({ appearance = "legacy" }: { appearance?: UiAppearance } = {}) {
  return <span data-ui-appearance={appearance} className={styles.workspaceCurrent}>Current area</span>;
}

function ChoiceBrand({ appearance = "legacy", brand }: ({ brand?: WorkspaceChoiceBrand }) & { appearance?: UiAppearance }) {
  if (!brand) return <Building2 size={17} aria-hidden="true" />;
  return (
    <span data-ui-appearance={appearance} className={styles.choiceBrand} aria-hidden="true">
      {brand.primaryUrl ? <img data-ui-appearance={appearance} src={brand.primaryUrl} alt="" /> : brand.initials}
    </span>
  );
}

export function WorkspaceMenu({ appearance = "legacy",
  label,
  choices,
  current,
  canManageErpx,
  defaultOpen = false,
  onDismiss,
}: ({
  label: string;
  choices: WorkspaceChoice[];
  current: CurrentWorkspaceChoice;
  canManageErpx: boolean;
  defaultOpen?: boolean;
  onDismiss?: () => void;
}) & { appearance?: UiAppearance }) {
  const ready = useHydrated();
  const pathname = usePathname();
  const [open, setOpen] = useState(defaultOpen);
  const [pendingCompany, setPendingCompany] = useState("");
  const [error, setError] = useState("");
  const switching = useRef(false);
  const holdings = choices.filter(
    (choice): choice is Extract<WorkspaceChoice, { kind: "holding" }> =>
      choice.kind === "holding" && choice.holdingSlug !== "erpx",
  );
  const platform = choices.find((choice) => choice.kind === "platform");
  const erpxManagementCurrent =
    pathname === canonicalUiHref("/holdings/erpx/manage-erpx") || pathname.startsWith(canonicalUiHref("/holdings/erpx/manage-erpx/"));

  const companyIsCurrent = (holdingSlug: string, company: WorkspaceCompanyChoice) =>
    current.kind === "holding" &&
    current.holdingSlug === holdingSlug &&
    current.companyId === company.id;
  const branchIsCurrent = (holdingSlug: string, companyId: string, branch: WorkspaceBranchChoice) =>
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

  async function selectCompany(holdingSlug: string, company: WorkspaceCompanyChoice) {
    if (!company.destination || !company.switchEndpoint) return;
    if (companyIsCurrent(holdingSlug, company)) {
      setOpen(false);
      return;
    }
    if (switching.current) return;
    switching.current = true;
    setPendingCompany(`${holdingSlug}:${company.id}`);
    setError("");
    try {
      await waitForWorkspaceMemory();
      await leavePushForWorkspaceChange(current, { holdingSlug, companyId: company.id });
      const response = await workFetch(company.switchEndpoint, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ companyId: company.id }),
      });
      if (!response.ok) {
        setError("Business could not be selected. Check your access and try again.");
        switching.current = false;
        setPendingCompany("");
        return;
      }
      window.location.assign(company.destination);
    } catch (error) {
      switching.current = false;
      setError(
        error instanceof Error
          ? error.message
          : "Business could not be selected. Check your connection and try again.",
      );
      setPendingCompany("");
    }
  }

  async function navigateWorkspaceLink(
    event: MouseEvent<HTMLAnchorElement>,
    holdingSlug: string | null,
    companyId: string | null,
    destination: string,
  ) {
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.altKey ||
      event.shiftKey
    )
      return;
    if (switching.current) {
      event.preventDefault();
      return;
    }
    if (
      current.kind === "platform" ||
      (current.holdingSlug === holdingSlug && current.companyId === companyId)
    ) {
      setOpen(false);
      return;
    }
    switching.current = true;
    event.preventDefault();
    setPendingCompany(`${holdingSlug}:${companyId}`);
    setError("");
    try {
      await leavePushForWorkspaceChange(current, { holdingSlug, companyId });
      window.location.assign(destination);
    } catch (error) {
      switching.current = false;
      setError(
        error instanceof Error
          ? error.message
          : "Could not turn off Push for the previous workspace. Retry the switch.",
      );
      setPendingCompany("");
    }
  }

  function departmentLink(
    holdingSlug: string,
    companyId: string,
    department: WorkspaceDepartmentChoice,
  ) {
    const isCurrent = departmentIsCurrent(holdingSlug, companyId, department);
    return (
      <a data-ui-appearance={appearance}
        key={department.id}
        className={`${styles.workspaceChoice} ${styles.workspaceDepartmentChoice}`}
        href={department.destination}
        aria-current={isCurrent ? "location" : undefined}
        onClick={(event) =>
          void navigateWorkspaceLink(event, holdingSlug, companyId, department.destination)
        }
      >
        <Network size={16} aria-hidden="true" />
        <span data-ui-appearance={appearance}>{department.label}</span>
        {isCurrent && <CurrentArea appearance={appearance} />}
      </a>
    );
  }

  return (
    <>
      <Button appearance={appearance}
        variant="plain"
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
        <span data-ui-appearance={appearance} className={styles.workspaceTriggerLabel}>{label}</span>
        <ChevronDown className={styles.workspaceTriggerChevron} size={14} aria-hidden="true" />
      </Button>
      <Dialog appearance={appearance}
        title="Switch workspace"
        open={open}
        onClose={() => {
          setOpen(false);
          onDismiss?.();
        }}
      >
        {canManageErpx && (
          <section data-ui-appearance={appearance} className={styles.workspaceSection}>
            <h3 data-ui-appearance={appearance}>ERPX</h3>
            <div data-ui-appearance={appearance} className={styles.workspaceTree}>
              <Link
                className={styles.workspaceChoice}
                href={canonicalUiHref("/holdings/erpx/manage-erpx")}
                aria-current={erpxManagementCurrent ? "location" : undefined}
                onClick={(event) =>
                  void navigateWorkspaceLink(event, "erpx", null, canonicalUiHref("/holdings/erpx/manage-erpx"))
                }
              >
                <ShieldCheck size={17} aria-hidden="true" />
                <span data-ui-appearance={appearance}>ERPX Management</span>
                {erpxManagementCurrent && <CurrentArea appearance={appearance} />}
              </Link>
            </div>
          </section>
        )}
        <section data-ui-appearance={appearance} className={styles.workspaceSection}>
          <h3 data-ui-appearance={appearance}>Business workspaces</h3>
          {!holdings.length && <p data-ui-appearance={appearance}>No business workspaces assigned.</p>}
          <div data-ui-appearance={appearance} className={styles.workspaceTree}>
            {holdings.map((holding) => (
              <section data-ui-appearance={appearance} key={holding.holdingSlug} className={styles.workspaceHolding}>
                <h4 data-ui-appearance={appearance}>
                  <ChoiceBrand appearance={appearance} brand={holding.brand} />
                  {holding.label}
                </h4>
                {holding.companies.map((company) => {
                  const companyCurrent = companyIsCurrent(holding.holdingSlug, company);
                  const pendingKey = `${holding.holdingSlug}:${company.id}`;
                  return (
                    <div data-ui-appearance={appearance} key={company.id} className={styles.workspaceCompany}>
                      {company.destination && company.switchEndpoint ? (
                        <Button appearance={appearance}
                          variant="plain"
                          type="button"
                          className={`${styles.workspaceChoice} ${styles.workspaceCompanyChoice}`}
                          aria-current={companyCurrent ? "location" : undefined}
                          aria-busy={pendingCompany === pendingKey}
                          disabled={!!pendingCompany}
                          onClick={() => selectCompany(holding.holdingSlug, company)}
                        >
                          <Building2 size={17} aria-hidden="true" />
                          <span data-ui-appearance={appearance}>{company.label}</span>
                          {companyCurrent && <CurrentArea appearance={appearance} />}
                        </Button>
                      ) : (
                        <div data-ui-appearance={appearance}
                          className={`${styles.workspaceChoice} ${styles.workspaceCompanyLabel}`}
                        >
                          <Building2 size={17} aria-hidden="true" />
                          <span data-ui-appearance={appearance}>{company.label}</span>
                        </div>
                      )}
                      {(company.branches.length > 0 || company.departments.length > 0) && (
                        <div data-ui-appearance={appearance} className={styles.workspaceLocations}>
                          {company.branches.map((branch) => {
                            const branchCurrent = branchIsCurrent(
                              holding.holdingSlug,
                              company.id,
                              branch,
                            );
                            return (
                              <div data-ui-appearance={appearance} key={branch.id}>
                                {branch.destination ? (
                                  <a data-ui-appearance={appearance}
                                    className={styles.workspaceChoice}
                                    href={branch.destination}
                                    aria-current={branchCurrent ? "location" : undefined}
                                    onClick={(event) =>
                                      void navigateWorkspaceLink(
                                        event,
                                        holding.holdingSlug,
                                        company.id,
                                        branch.destination!,
                                      )
                                    }
                                  >
                                    <ChoiceBrand appearance={appearance} brand={branch.brand} />
                                    <span data-ui-appearance={appearance}>{branch.label}</span>
                                    {branchCurrent && <CurrentArea appearance={appearance} />}
                                  </a>
                                ) : (
                                  <div data-ui-appearance={appearance}
                                    className={`${styles.workspaceChoice} ${styles.workspaceBranchLabel}`}
                                  >
                                    <ChoiceBrand appearance={appearance} brand={branch.brand} />
                                    <span data-ui-appearance={appearance}>{branch.label}</span>
                                  </div>
                                )}
                                {branch.departments.length > 0 && (
                                  <div data-ui-appearance={appearance} className={styles.workspaceDepartments}>
                                    {branch.departments.map((department) =>
                                      departmentLink(holding.holdingSlug, company.id, department),
                                    )}
                                  </div>
                                )}
                              </div>
                            );
                          })}
                          {company.departments.map((department) =>
                            departmentLink(holding.holdingSlug, company.id, department),
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
            <p data-ui-appearance={appearance} className={styles.workspaceError} role="alert">
              {error}
            </p>
          )}
        </section>
        {platform && (
          <section data-ui-appearance={appearance} className={styles.workspaceSection}>
            <h3 data-ui-appearance={appearance}>Platform administration</h3>
            <div data-ui-appearance={appearance} className={styles.workspaceTree}>
              <a data-ui-appearance={appearance}
                className={styles.workspaceChoice}
                href={platform.destination}
                aria-current={current.kind === "platform" ? "location" : undefined}
                onClick={(event) =>
                  void navigateWorkspaceLink(event, null, null, platform.destination)
                }
              >
                <ShieldCheck size={17} aria-hidden="true" />
                <span data-ui-appearance={appearance}>{platform.label}</span>
                {current.kind === "platform" && <CurrentArea appearance={appearance} />}
              </a>
            </div>
          </section>
        )}
      </Dialog>
    </>
  );
}
