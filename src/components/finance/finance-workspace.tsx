import { FinanceChrome, type FinanceChromeProps } from "./finance-chrome";
import styles from "./finance-workspace.module.css";

export function FinanceWorkspace(props: FinanceChromeProps) {
  return <FinanceChrome {...props}/>;
}

export const financeStyles = styles;
