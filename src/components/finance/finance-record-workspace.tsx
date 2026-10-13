"use client";

import { useEmbeddedRecord } from "@/components/records/record-context";
import { financeNavigation } from "@/lib/finance/finance-navigation";
import { useFeatureNavigation } from "@/components/shared/features/feature-environment";
import { FinanceChrome, type FinanceChromeProps } from "./finance-chrome";

/** Detail pages keep their record widgets and authority; only their chrome changes. */
export function FinanceRecordWorkspace(props: FinanceChromeProps) {
  const context = useFeatureNavigation(), embedded = useEmbeddedRecord();
  return !embedded && context.financeWorkspaceV2 && financeNavigation(context).length > 0 ? <FinanceChrome {...props}/> : <>{props.children}</>;
}

export { FinanceLegacyHeading } from "./finance-legacy-heading";
