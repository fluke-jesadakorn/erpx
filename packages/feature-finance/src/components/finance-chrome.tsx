"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";

import { Button } from "@erpx/ui/components/foundation";

import { usePathname } from "next/navigation";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import type { ReactNode } from "react";
import { Settings2, Sparkles } from "lucide-react";
import { useFeatureNavigation } from "@erpx/feature-shared/components/features/feature-page";
import {
  financeAreaForDestination,
  financeAreas,
  financeDestinations,
  financeDestinationForRoute,
  financeNavigation,
  financeSidebarId,
} from "@erpx/feature-finance/lib/finance-navigation";
import { FinanceAiPanel } from "@erpx/feature-finance/components/finance-ai-panel";
import styles from "@erpx/feature-finance/components/finance-workspace.module.css";
import { FinancePurchaseProvider } from "@erpx/feature-finance/components/purchase-context";
import { financeActionAllowed, type FinancePurchaseControls } from "@erpx/server-finance/public/rules/purchase-controls";

const legacyActive: Record<string, string> = {
  work: "finance-overview",
  receivables: "finance-receivables",
  expenses: "finance-expenses",
  "finance-expenses": "finance-expenses",
  payables: "finance-payables",
  cash: "finance-bank",
  hotels: "finance-hotels",
  accounting: "finance-ledger",
  tax: "finance-tax",
  close: "finance-close",
  reports: "finance-reports",
  engines: "finance-engines",
  group: "finance-group",
  setup: "finance-setup",
};

export type FinanceChromeProps = {
  base: string;
  active: string;
  title: string;
  description: string;
  actions?: ReactNode;
  actionsPurpose?: "new-work" | "settle-existing" | "history";
  purchases?: FinancePurchaseControls;
  sourceHistory?: boolean;
  selectedPeriodId?: string;
  filingMonth?: string;
  context?: {
    chartId: string;
    chartName: string;
    currency: string;
    periodName?: string;
    periodId?: string;
  };
  children: ReactNode;
};

export function FinanceChrome({
  base,
  active,
  title,
  description,
  actions: suppliedActions,
  actionsPurpose,
  purchases,
  sourceHistory,
  selectedPeriodId,
  filingMonth,
  context: financialContext,
  children,
}: FinanceChromeProps) {
  const actions = !actionsPurpose || financeActionAllowed(purchases, actionsPurpose) ? suppliedActions : undefined;
  const context = useFeatureNavigation();
  const pathname = usePathname();
  const query = useSearchParams();
  if (!context.financeWorkspaceV2)
    return (
      <FinancePurchaseProvider purchases={purchases} sourceHistory={sourceHistory}><div className={styles.shell}>
        <header className={styles.header}>
          <div>
            <p className="eyebrow">Finance workspace</p>
            <h1>{title}</h1>
            <p className="muted">{description}</p>
          </div>
          {actions}
        </header>
        {children}
      </div></FinancePurchaseProvider>
    );

  const items = financeNavigation(context);
  const presenterDestination = financeDestinations.find(entry => entry.id === active)?.id ?? legacyActive[active] ?? financeDestinationForRoute(active, query.get("kind"));
  const routeDestination = financeDestinations.map(entry => ({entry, path: new URL(canonicalUiHref(`${base}/${entry.route}`), "https://finance.invalid").pathname}))
    .filter(({path}) => pathname === path || pathname.startsWith(path + "/"))
    .sort((left, right) => right.path.length - left.path.length)[0]?.entry.id;
  const resolved = {activeId: routeDestination ?? presenterDestination, sidebarId: ""};
  if (/\/finance\/(?:invoices|documents\/(?:invoices|receivable|payable))(?:\/|$)/.test(pathname) && ["receivables", "payables"].includes(active))
    resolved.activeId = active === "payables" ? "finance-payables" : "finance-receivables";
  resolved.sidebarId = financeSidebarId(resolved.activeId);
  const area = financeAreaForDestination(resolved.activeId);
  const areaLabel = financeAreas.find((entry) => entry.id === area)?.label ?? "Setup";
  const setup = items.find((item) => item.id === "finance-setup");

  return (
    <FinancePurchaseProvider purchases={purchases} sourceHistory={sourceHistory}><div
      className={`${styles.shell} ${styles.modern}`}
      data-finance-workspace="v2"
      data-finance-area={area}
    >
      <header className={styles.header}>
        <div>
          <p className="eyebrow">Finance / {areaLabel}</p>
          <h1>{title}</h1>
          <p className="muted">{description}</p>
          {financialContext && (
            <p className={styles.context} aria-label="Financial context">
              <span>{financialContext.chartName}</span>
              <span>Functional currency {financialContext.currency}</span>
              {financialContext.periodName && <span>{financialContext.periodName}</span>}
            </p>
          )}
        </div>
        <div className={styles.headerActions}>
          {actions}
          <Button appearance="standard"
            variant="secondary"
            type="button"
            onClick={() =>
              window.dispatchEvent(
                new CustomEvent("erpx:open-assistant", { detail: { mode: "chat" } }),
              )
            }
          >
            <Sparkles size={16} aria-hidden="true" /> Ask ERPX
          </Button>
          {setup && area !== "setup" && (
            <UiActionLink appearance="standard"
              variant="ghost"
              className={styles.iconAction}
              href={canonicalUiHref(setup.href)}
              aria-label="Finance setup"
              title="Finance setup"
            >
              <Settings2 size={18} aria-hidden="true" />
            </UiActionLink>
          )}
        </div>
      </header>
      {(context.scopeKind ?? "holding") === "holding" && area !== "setup" && (
        <FinanceAiPanel
          base={base}
          chartId={financialContext?.chartId}
          active={active}
          periodId={selectedPeriodId ?? financialContext?.periodId}
          filingMonth={filingMonth}
        />
      )}
      {children}
    </div></FinancePurchaseProvider>
  );
}
