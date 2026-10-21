"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { RecordLink as Link } from "@/components/records/record-link";
import { usePathname } from "next/navigation";
import { useSearchParams } from "@/lib/shared/destination-search";
import type { ReactNode } from "react";
import { ChevronDown, Settings2, Sparkles } from "lucide-react";
import { useFeatureNavigation } from "@/components/shared/features/feature-page";
import {
  financeAreaForDestination,
  financeAreaItems,
  financeAreas,
  financeContextHref,
  financeDestinations,
  financeDestinationForRoute,
  financeNavigation,
  financeRootItems,
  financeSidebarId,
} from "@/lib/finance/finance-navigation";
import { FinanceAiPanel } from "./finance-ai-panel";
import styles from "./finance-workspace.module.css";
import { FinancePurchaseProvider } from "./purchase-context";
import { financeActionAllowed, type FinancePurchaseControls } from "@/server/modules/finance/public/rules/purchase-controls";
import { hasPermission } from "@/lib/identity_access/permissions";

const legacyIds = [
  "finance-overview",
  "finance-sales",
  "finance-receivables",
  "finance-expenses",
  "finance-payables",
  "finance-bank",
  "finance-hotels",
  "finance-ledger",
  "finance-tax",
  "finance-close",
  "finance-reports",
  "finance-engines",
  "finance-group",
  "finance-setup",
];
const legacyLabels: Record<string, string> = {
  "finance-overview": "My work",
  "finance-expenses": "Employee expenses",
  "finance-bank": "Cash & banking",
  "finance-hotels": "Hotel accounting",
  "finance-ledger": "Accounting",
  "finance-tax": "Tax",
  "finance-reports": "Reports",
  "finance-engines": "Calculation engines",
  "finance-group": "Group reporting",
  "finance-setup": "Setup",
};
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
  const expenseNavigationActor = { role: context.role ?? "employee", permissions: context.permissions, assignmentScope: context.scopeKind };
  const canReadExpenses = hasPermission(expenseNavigationActor, "expense.read") || hasPermission(expenseNavigationActor, "expense.review.read");
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
        <nav className={styles.nav} aria-label="Finance work areas">
          {legacyIds.filter(id => id !== "finance-expenses" || canReadExpenses).map((id) => {
            const destination = financeDestinations.find((entry) => entry.id === id)!;
            return (
              <Link
                key={id}
                href={canonicalUiHref(`${base}/${destination.route}`)}
                data-active={legacyActive[active] === id || undefined}
              >
                {legacyLabels[id] ?? destination.label}
              </Link>
            );
          })}
        </nav>
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
  const tabs = financeAreaItems(items, area);
  const roots = financeRootItems(items);
  const areaLabel = financeAreas.find((entry) => entry.id === area)?.label ?? "Setup";
  const tools = items.filter((item) =>
    financeDestinations.some(
      (destination) => destination.id === item.id && destination.area === area && destination.tool,
    ),
  );
  const setup = items.find((item) => item.id === "finance-setup");
  const linkContext = new URLSearchParams(query.toString());
  if (financialContext) linkContext.set("chart", financialContext.chartId);
  const href = (value: string) => financeContextHref(value, linkContext);

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
              href={href(setup.href)}
              aria-label="Finance setup"
              title="Finance setup"
            >
              <Settings2 size={18} aria-hidden="true" />
            </UiActionLink>
          )}
        </div>
      </header>
      <details className={styles.areaPicker}>
        <summary>
          {areaLabel}
          <ChevronDown size={16} aria-hidden="true" />
        </summary>
        <nav aria-label="Finance areas">
          {roots.map((item) => (
            <Link
              href={href(item.href)}
              key={item.id}
              aria-current={resolved.sidebarId === item.id ? "page" : undefined}
            >
              {item.label}
            </Link>
          ))}
        </nav>
      </details>
      {(tabs.length > 1 || tools.length > 0) && (
        <div className={styles.localBar}>
          <nav className={styles.localNav} aria-label={`${areaLabel} views`}>
            {tabs.map((item) => (
              <Link
                href={href(item.href)}
                key={item.id}
                aria-current={resolved.activeId === item.id ? "page" : undefined}
              >
                {item.label}
              </Link>
            ))}
          </nav>
          {tools.length > 0 && (
            <details className={styles.tools}>
              <summary>
                {area === "accounting" ? "Accounting tools" : area === "cash" ? "Payment tools" : `${areaLabel} tools`}
                <ChevronDown size={15} aria-hidden="true" />
              </summary>
              <div>
                {tools.map((item) => (
                  <Link href={href(item.href)} key={item.id}>
                    {item.label}
                  </Link>
                ))}
              </div>
            </details>
          )}
        </div>
      )}
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
