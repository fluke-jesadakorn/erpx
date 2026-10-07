import { pmsPresentationQueries } from "@/server/modules/pms/public/presentation-queries";
import {recordCanonicalHref} from "@/lib/record-drawer";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Table, TableFrame } from "@/components/ui/foundation";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@/components/features/feature-page";
import { money, type Entry, type Folio, type Guest, type PmsRecord } from "@/server/modules/pms/public/rules/pms-model";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";

import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";



import { RecordLink as Link } from "@/components/records/record-link";
import { notFound } from "next/navigation";
import styles from "@/components/pms/document.module.css";
export const dynamic = "force-dynamic";
export default async function PmsDocument({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = requirePermission(await requireWorkContext(holdingSlug), "pms.read"),
    presentation = await pmsPresentationQueries(actor.holdingId);
  const { hasPermission } = await import("@/lib/permissions");
  if (!hasPermission(actor, "pms.cashier") && !hasPermission(actor, "pms.reports")) notFound();
  const queries = presentation.pmsDocumentQueries();
  const doc = await queries.document(id);
  if (!doc) notFound();
  const s = await presentation.loadPms(actor, doc.property_id);
  const postings = await queries.relatedPostings(doc.property_id, id);
  const snap = JSON.parse(doc.snapshot_json) as {
    property: { name: string; currency: string };
    guest: Guest;
    folio: Folio;
    entries: PmsRecord<"entry">[];
    amount: number;
    businessDate: string;
    openingJournalId?: string;
  };
  const brand = await presentation.resolveDocumentBrand(actor, {
    branchId: s.property.branchId,
  });
  const titles: Record<string, string> = {
    invoice: "Invoice / expense summary",
    receipt: "Receipt",
    refund: "Refund evidence",
    credit_note: "Credit note",
    opening: "Opening balance evidence / หลักฐานยอดยกมา",
  };
  const bill = (e: Entry) => ["charge", "adjustment", "transfer"].includes(e.kind);
  return (
    <FeaturePage className="content-stack">
      <header className="section-head">
        <div>
          <p className="eyebrow">{snap.property.name}</p>
          <h1>{titles[doc.kind]}</h1>
          <p>
            {brand.companyName} · {snap.businessDate}
          </p>
        </div>
        <UiActionLink
          variant="secondary"

          href={`${workspaceBase(publicScope(actor))}/pms/folios?property=${s.property.id}`}
        >
          Back to bill
        </UiActionLink>
      </header>
      <FeatureTabs />
      <WidgetGrid pageKey="pms-documents-detail">
        <FeatureWidget id="pms-folios-document" title="Document">
          <nav className={styles.sourceLinks} aria-label="Document sources">
            <UiActionLink
              variant="secondary"
              href={`${workspaceBase(publicScope(actor))}/pms/folios?property=${s.property.id}&folio=${doc.folio_id}`}
            >
              Guest account
            </UiActionLink>
            {doc.finance_id && hasPermission(actor, "finance.read") && (
              <UiActionLink
                variant="secondary"
                href={`${workspaceBase(publicScope(actor))}/finance/${["receipt", "refund"].includes(doc.kind) || (doc.kind === "opening" && snap.entries[0]?.data.kind !== "charge") ? "payments" : "invoices"}/${doc.finance_id}`}
              >
                Finance document
              </UiActionLink>
            )}
          </nav>
          {doc.kind === "opening" && (
            <p>
              Balances carried from the previous system. This evidence records no new cash receipt
              or historical revenue.{" "}
              {snap.openingJournalId && hasPermission(actor, "finance.read") && (
                <Link
                  href={`${workspaceBase(publicScope(actor))}/finance/journals/${snap.openingJournalId}`}
                >
                  Approved opening journal →
                </Link>
              )}
            </p>
          )}
          {hasPermission(actor, "finance.read") && postings.length > 0 && (
            <nav className={styles.sourceLinks} aria-label="Related journals">
              {postings.map((posting) => (
                <Link
                  className={styles.journalLink}
                  key={posting.journal_id}
                  href={`${workspaceBase(publicScope(actor))}/finance/journals/${posting.journal_id}`}
                >
                  Journal {posting.journal_id.slice(-8)} →
                </Link>
              ))}
            </nav>
          )}
          <p className={styles.reference}>PMS-{id}</p>
          <h2>{snap.guest.name}</h2>
          <p>{[snap.guest.email, snap.guest.phone].filter(Boolean).join(" · ")}</p>
          <p>{snap.folio.name}</p>
          <TableFrame label="Records" className="business-table-wrap">
            <Table className={`business-table ${styles.entries}`}>
              <thead>
                <tr>
                  <th>Date</th>
                  <th>Description</th>
                  <th>Net</th>
                  <th>Service Charge</th>
                  <th>Tax</th>
                  <th>Total</th>
                </tr>
              </thead>
              <tbody>
                {snap.entries
                  .filter((e) => doc.kind !== "invoice" || bill(e.data))
                  .map((e) => (
                    <tr key={e.id}>
                      <td data-label="Date">{e.data.date}</td>
                      <td data-label="Description"><Link href={recordCanonicalHref(workspaceBase(publicScope(actor)),{kind:"pms_snapshot_entry",id:e.id,args:{documentId:id}})}>{e.data.description}</Link></td>
                      <td data-label="Net">{money(e.data.net)}</td>
                      <td data-label="Service charge">{money(e.data.service)}</td>
                      <td data-label="Tax">{money(e.data.tax)}</td>
                      <td data-label="Total">{money(e.data.total)}</td>
                    </tr>
                  ))}
              </tbody>
            </Table>
          </TableFrame>
          <h2>Total {money(snap.amount, snap.property.currency)}</h2>
          <p>Balance due at issue {money(snap.folio.due)}</p>
          <p className="muted">
            Summary document from the PMS · Use your browser&apos;s print command to print or save a
            PDF
          </p>
        </FeatureWidget>
      </WidgetGrid>
    </FeaturePage>
  );
}
