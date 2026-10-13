"use client";
import { recordCanonicalHref } from "@/lib/records/record-drawer";
import { Table, TableFrame } from "@/components/ui/foundation";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import Link from "next/link";
import { type ReactNode } from "react";
import dynamic from "next/dynamic";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {BackofficePmsWorkspaceContext} from "./pms-backoffice-workspace";


const PmsRangeReports = dynamic(() => import("./range-reports").then(module => module.PmsRangeReports));


const Panel = ({ title, children }: { title: string; children: ReactNode }) => {
  const language = useFrontLanguage();
  return (
    <FeatureWidget
      id={
        "pms-" +
        title
          .toLowerCase()
          .replace(/[^a-z0-9]+/g, "-")
          .replace(/^-|-$/g, "")
      }
      title={frontText(title, language)}
    >
      {children}
    </FeatureWidget>
  );
};
export function BackofficePmsWorkspaceReportsScreen({context}: {context: BackofficePmsWorkspaceContext & {p: NonNullable<BackofficePmsWorkspaceContext["p"]>}}) {
  const {crossLinks, erpBase, base, p, today, data, rows, money, dailyPmsHref} = context;
  return <FrontLocalizedTree>{(
                <>
                  {crossLinks.finance && (
                    <p className={styles.muted}>
                      Review shared accounting in{" "}
                      <Link href={`${erpBase}/finance/accounting`}>Finance Accounting</Link>.
                    </p>
                  )}
                  <PmsRangeReports base={base} propertyId={p.id} today={today}/>
                  <Panel title="PMS vs GL">
                    <TableFrame label="Records" className={styles.tableWrap}>
                      <Table>
                        <thead>
                          <tr>
                            <th>Control account</th>
                            <th>PMS</th>
                            <th>GL</th>
                            <th>Variance</th>
                          </tr>
                        </thead>
                        <tbody>
                          {(["due", "advance", "deposit"] as const).map((key) => {
                            const n = data.controlBalances?.[key] ?? rows("guest_account").reduce((n, f) => n + f.data[key], 0),
                              gl = data.ledger?.[key === "due" ? "receivable" : key] ?? 0;
                            return (
                              <tr key={key}>
                                <th>
                                  <Link href={recordCanonicalHref(erpBase,{kind:'pms_control_sources',id:p.id,args:{control:key==='due'?'receivable':key}})}>{key === "due" ? "Receivable" : key === "advance" ? "Advance" : "Deposit"}</Link>
                                </th>
                                <td>{money(n)}</td>
                                <td>{money(gl)}</td>
                                <td>{money(n - gl)}</td>
                              </tr>
                            );
                          })}
                        </tbody>
                      </Table>
                    </TableFrame>
                    <p>
                      Debit {money(data.ledger?.debit ?? 0)} · Credit{" "}
                      {money(data.ledger?.credit ?? 0)}
                    </p>
                  </Panel>
                  <Panel title="Receivables & balances">
                    <Link href={dailyPmsHref("guest-accounts", { property:p.id })}>View guest accounts and balances</Link>
                  </Panel>
                </>
              )}</FrontLocalizedTree>;
}
