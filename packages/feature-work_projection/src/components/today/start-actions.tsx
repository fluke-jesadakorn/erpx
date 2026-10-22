import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { StartPrompt } from "@erpx/feature-work_projection/components/today/start-prompt";

import Link from "@erpx/workplace/components/workspace-link";
import {
  CalendarDays,
  CheckSquare2,
  CirclePlus,
  MessageSquareText,
  ReceiptText,
  ShoppingBasket,
  Send,
  Scale,
  Clock3,
  LifeBuoy,
} from "lucide-react";
import styles from "@erpx/feature-work_projection/components/today/start-actions.module.css";
import { workplaceStartActions } from "@erpx/feature-work_projection/lib/workplace-catalogue";

const actionIcons = {
  buy: ShoppingBasket,
  expense: ReceiptText,
  leave: CalendarDays,
  overtime: Clock3,
  task: CheckSquare2,
  post: MessageSquareText,
  handover: Send,
  help: LifeBuoy,
  plan: CalendarDays,
};
const actions = workplaceStartActions.map((action) => ({
  ...action,
  icon: actionIcons[action.id],
}));

export function StartActions({
  base,
  compact = false,
  rail = false,
  onNavigate,
  canRequestLegal = false,
}: {
  base: string;
  compact?: boolean;
  rail?: boolean;
  onNavigate?: () => void;
  canRequestLegal?: boolean;
}) {
  return (
    <section
      className={`${styles.start} ${compact ? styles.compact : ""} ${rail ? styles.rail : ""}`}
      id={compact ? undefined : "start"}
      aria-label="Start work"
    >
      {!compact && (
        <div className={styles.heading}>
          <span className={styles.eyebrow}>
            <CirclePlus size={16} aria-hidden="true" /> START SOMETHING
          </span>
          <h2>What would you like to do?</h2>
          <p>Tell ERPX what you need, or choose a direct path.</p>
        </div>
      )}
      <StartPrompt rail={rail} onNavigate={onNavigate} />
      <div className={styles.actions}>
        {canRequestLegal && (
          <Link href={canonicalUiHref(`${base}/legal?kind=request&new=request`)} onClick={onNavigate}>
            <Scale size={18} aria-hidden="true" />
            Request legal advice
          </Link>
        )}
        {actions.map(({ id, label, icon: Icon, path }) => (
          <Link key={id} href={`${base}${path}`} onClick={onNavigate}>
            <Icon size={18} aria-hidden="true" />
            {label}
          </Link>
        ))}
      </div>
    </section>
  );
}
