import { FinanceChrome } from "@erpx/feature-records/components/loaders/finance--finance-chrome";
import type { FinanceChromeProps } from "@erpx/feature-finance/components/finance-chrome";
import styles from "@erpx/feature-finance/components/finance-workspace.module.css";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { financePurchaseControls } from "@erpx/server-finance/public/purchases";

export async function FinanceWorkspace({ actor, ...props }: FinanceChromeProps & { actor?: WorkContext }) {
  const purchases = actor ? await financePurchaseControls(actor) : undefined;
  return <FinanceChrome {...props} purchases={purchases}/>;
}

export const financeStyles = styles;
