import { FinanceChrome } from "@erpx/records-ui/components/loaders/finance--finance-chrome";
import type { FinanceChromeProps } from "@erpx/finance-ui/components/finance-chrome";
import styles from "@erpx/finance-ui/components/finance-workspace.module.css";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { financePurchaseControls } from "@erpx/finance-server/public/purchases";

export async function FinanceWorkspace({ actor, ...props }: FinanceChromeProps & { actor?: WorkContext }) {
  const purchases = actor ? await financePurchaseControls(actor) : undefined;
  return <FinanceChrome {...props} purchases={purchases}/>;
}

export const financeStyles = styles;
