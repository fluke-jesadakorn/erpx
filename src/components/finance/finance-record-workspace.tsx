"use client";

import { financeNavigation } from "@/lib/finance-navigation";
import type { ReactNode } from "react";
import { useFeatureNavigation } from "@/components/features/feature-page";
import { FinanceChrome, type FinanceChromeProps } from "./finance-chrome";

/** Detail pages keep their record widgets and authority; only their chrome changes. */
export function FinanceRecordWorkspace(props: FinanceChromeProps) {
  const context = useFeatureNavigation();
  return context.financeWorkspaceV2 && financeNavigation(context).length > 0 ? <FinanceChrome {...props}/> : <>{props.children}</>;
}

export function FinanceLegacyHeading({ children }: { children: ReactNode }) {
  const context = useFeatureNavigation();
  return context.financeWorkspaceV2 && financeNavigation(context).length > 0 ? null : <>{children}</>;
}
