import Link from "next/link";

import type { PurchaseRequestProgress as Progress } from "@/server/purchase-request-progress";
import styles from "./purchase-request-progress.module.css";
import { Card } from "@/components/ui/foundation";

export function PurchaseRequestProgress({
  base,
  progress,
}: {
  base: string;
  progress: Progress;
}) {
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <p className="eyebrow">
        {progress.completed} of {progress.total} stages complete
      </p>
      <ol className={styles.list} aria-label="Purchase-to-payment progress">
        {progress.steps.map((step) => (
          <li key={step.id} data-state={step.state}>
            <div>
              <strong>{step.label}</strong>
              <p>{step.detail}</p>
            </div>
            <span>{step.state}</span>
            {step.href && <Link href={`${base}${step.href}`}>Open</Link>}
          </li>
        ))}
      </ol>
      <p className="muted">
        Each stage remains owned by its department. This view follows their
        source records and does not copy approvals, receipts, invoices, or
        payments into the request.
      </p>
    </Card>
  );
}
