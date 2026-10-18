"use client";
import { createContext, useContext, type ReactNode } from "react";
import { InlineNotice } from "@/components/ui/foundation";
import { financeActionAllowed, type FinancePurchaseControls } from "@/server/modules/finance/public/rules/purchase-controls";
import { FinanceDraftBoundary } from "./finance-draft-boundary";
const Context = createContext<FinancePurchaseControls | undefined>(undefined);
export function FinancePurchaseProvider({ purchases, sourceHistory, children }: { purchases?: FinancePurchaseControls; sourceHistory?:boolean; children: ReactNode }) {
  const access = purchases?.["finance.accounting"];
  return <Context.Provider value={purchases}>
    {access && !access.newWork && <InlineNotice tone="info">{sourceHistory&&!access.history ? "This source record and its retained history remain available through the original product." : access.settleExisting ? "Existing Finance work and history remain available." : access.history ? "Finance history and exports remain available." : "Choose an available Finance workspace from Apps."}</InlineNotice>}
    <FinanceDraftBoundary>{children}</FinanceDraftBoundary>
  </Context.Provider>;
}
export function useFinanceActionAllowed(purpose: "new-work" | "settle-existing" | "history", capability: string | readonly string[] = "finance.accounting") {
  const purchases = useContext(Context);
  return (typeof capability === "string" ? [capability] : capability).some(value => financeActionAllowed(purchases, purpose, value));
}
