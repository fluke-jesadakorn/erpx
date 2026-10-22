import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import Link from "@erpx/workplace/components/workspace-link";
import type { WorkActionCard, WorkCapabilities } from "@erpx/feature-work_projection/lib/work-cards";
import { selfServiceActions } from "@erpx/feature-work_projection/lib/self-service";
import styles from "@erpx/workplace/components/work-dashboard.module.css";

export function ForMe({ base, cards, capabilities }: { base: string; cards: WorkActionCard[]; capabilities: WorkCapabilities }) {
  const personal = cards.filter(card => ["shifts", "leave", "overtime", "expenses", "benefits", "learning"].includes(card.id));
  return <section aria-label="For me">
    <p className="muted">Your schedule, time off, expense claims and personal services available in this workspace.</p>
    <div className={styles.personal}>
      <Link className="ui-panel" href={canonicalUiHref(`${base}/calendar`)}><strong>My calendar</strong><p>Appointments, shifts and work deadlines.</p></Link>
      {personal.map(card => <Link className="ui-panel" key={card.id} href={card.id === "expenses" ? canonicalUiHref(`${base}/work?view=tracking&tracking=requests&type=expense`) : ["benefits","learning"].includes(card.id) ? `${card.href}?view=mine` : card.href}>
        <strong>{card.id === "benefits" ? "My benefits" : card.id === "learning" ? "My learning" : card.title}</strong>
        <p>{selfServiceActions.find(action => action.id === card.id)?.description ?? (card.id === "expenses" ? "Your claims, review progress and payment history." : card.id === "benefits" ? "Your elections, enrollment and provider confirmation." : card.id === "learning" ? "Your assigned courses, progress and recorded results." : card.objective)}</p>
      </Link>)}
    </div>
    {!capabilities.creation.some(action => action.kind === "leave") && <p className="muted">Connect your account to an active staff profile to use personal leave and overtime.</p>}
  </section>;
}
