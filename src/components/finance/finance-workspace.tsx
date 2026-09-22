import Link from "next/link";
import type { ReactNode } from "react";
import styles from "./finance-workspace.module.css";

const sections = [
  ["work", "My work", "finance"],
  ["receivables", "Receivables", "finance/invoices?kind=receivable&queue=open"],
  ["payables", "Payables", "finance/invoices?kind=payable&queue=open"],
  ["cash", "Cash & banking", "finance/bank"],
  ["accounting", "Accounting", "finance/accounting"],
  ["close", "Period close", "finance/close"],
  ["reports", "Reports", "finance/reports"],
  ["engines", "Calculation engines", "finance/engines"],
  ["group", "Group reporting", "finance/group"],
  ["setup", "Setup", "finance/setup"],
] as const;

export function FinanceWorkspace({ base, active, title, description, actions, children }: { base: string; active: string; title: string; description: string; actions?: ReactNode; children: ReactNode }) {
  return <div className={styles.shell}>
    <header className={styles.header}><div><p className="eyebrow">Finance workspace</p><h1>{title}</h1><p className="muted">{description}</p></div>{actions}</header>
    <nav className={styles.nav} aria-label="Finance work areas">
      {sections.map(([id, label, route]) => <Link key={id} href={`${base}/${route}`} data-active={active === id || undefined}>{label}</Link>)}
    </nav>
    {children}
  </div>;
}

export const financeStyles = styles;
