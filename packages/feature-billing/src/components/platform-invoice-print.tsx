"use client";

import { baht, type BillingInvoice, type InvoiceLine } from "@erpx/feature-billing/lib/billing";
import { APP_LOCALE, APP_TIMEZONE } from "@erpx/feature-shared/lib/localization";
import { Button } from "@erpx/ui/components/button";
import styles from "./platform-invoice-print.module.css";

function date(value: number) {
  return new Intl.DateTimeFormat(APP_LOCALE, { timeZone: APP_TIMEZONE, dateStyle: "medium" }).format(value);
}

export function PlatformInvoicePrint({ customer, invoice, lines }: {
  customer: string;
  invoice: BillingInvoice;
  lines: InvoiceLine[];
}) {
  return <article className={styles.document} data-print-document>
    <header className={styles.header}>
      <div>
        <p>ERPX · Platform Billing</p>
        <h1>Service invoice</h1>
        <strong>{invoice.stripe_id}</strong>
      </div>
      <Button type="button" appearance="standard" className="no-print" onClick={() => window.print()}>
        Save as PDF
      </Button>
    </header>
    <p>Sandbox · Test invoice</p>
    <dl className={styles.facts}>
      <div><dt>Billed to</dt><dd>{customer}</dd></div>
      <div><dt>Issued</dt><dd>{date(invoice.created_at)}</dd></div>
      {invoice.due_at !== null && <div><dt>Due</dt><dd>{date(invoice.due_at)}</dd></div>}
      <div><dt>Status</dt><dd>{invoice.status}</dd></div>
    </dl>
    <table className={styles.lines}>
      <thead><tr><th scope="col">Service</th><th scope="col">Period</th><th scope="col">Amount</th></tr></thead>
      <tbody>{lines.map((line, index) => <tr key={index}>
        <td>{line.description}</td><td>{date(line.start)} – {date(line.end - 1)}</td><td>{baht(line.amount)}</td>
      </tr>)}</tbody>
      <tfoot><tr><th colSpan={2} scope="row">Total</th><td>{baht(invoice.total_satang)}</td></tr></tfoot>
    </table>
  </article>;
}
