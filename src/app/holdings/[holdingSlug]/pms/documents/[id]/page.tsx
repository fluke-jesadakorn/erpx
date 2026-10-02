import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import {
money,
type Entry,
type Folio,
type Guest,
type PmsRecord,
} from "@/lib/pms-model";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { resolveDocumentBrand } from "@/server/branding";
import { requirePermission } from "@/server/iam-permissions";
import { loadPms } from "@/server/pms";
import { holdingResources } from "@/server/runtime";
import Link from "next/link";
import { notFound } from "next/navigation";
import styles from "@/components/pms/document.module.css";
export const dynamic = "force-dynamic";
export default async function PmsDocument({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = requirePermission(
      await requireWorkContext(holdingSlug),
      "pms.read",
    ),
    { write } = await holdingResources(actor.holdingId);
  const { hasPermission } = await import("@/lib/permissions");
  if (
    !hasPermission(actor, "pms.cashier") &&
    !hasPermission(actor, "pms.reports")
  )
    notFound();
  const doc = await write
    .prepare("SELECT * FROM pms_documents WHERE id=?")
    .bind(id)
    .first<{
      property_id: string;
      folio_id:string;
      kind: string;
      snapshot_json: string;
      created_at: number;
      finance_id: string|null;
    }>();
  if (!doc) notFound();
  const s = await loadPms(write, actor, doc.property_id);
  const postings=(await write.prepare(`SELECT DISTINCT pp.journal_id FROM pms_postings pp JOIN pms_records r ON r.id=pp.record_id AND r.property_id=pp.property_id
    WHERE pp.property_id=? AND r.kind='entry' AND r.data_json::jsonb->>'folioId'=(SELECT folio_id FROM pms_documents WHERE id=?)`)
    .bind(doc.property_id,id).all<{journal_id:string}>()).results;
  const snap = JSON.parse(doc.snapshot_json) as {
    property: { name: string; currency: string };
    guest: Guest;
    folio: Folio;
    entries: PmsRecord<"entry">[];
    amount: number;
    businessDate: string;
    openingJournalId?:string;
  };
  const brand = await resolveDocumentBrand(write, actor, {
    branchId: s.property.branchId,
  });
  const titles: Record<string, string> = {
    invoice: "Invoice / expense summary",
    receipt: "Receipt",
    refund: "Refund evidence",
    credit_note: "Credit note",
    opening:"Opening balance evidence / หลักฐานยอดยกมา",
  };
  const bill = (e: Entry) =>
    ["charge", "adjustment", "transfer"].includes(e.kind);
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
        <Link
          className="button secondary"
          href={`${workspaceBase(publicScope(actor))}/pms/folios?property=${s.property.id}`}
        >
          Back to bill
        </Link>
      </header><FeatureTabs/><WidgetGrid pageKey="pms-documents-detail">
      <FeatureWidget id="pms-folios-document" title="Document">
        <nav className={styles.sourceLinks} aria-label="Document sources"><Link className="button secondary" href={`${workspaceBase(publicScope(actor))}/pms/folios?property=${s.property.id}&folio=${doc.folio_id}`}>Guest account</Link>
          {doc.finance_id&&hasPermission(actor,"finance.read")&&<Link className="button secondary" href={`${workspaceBase(publicScope(actor))}/finance/${["receipt","refund"].includes(doc.kind)||doc.kind==="opening"&&snap.entries[0]?.data.kind!=="charge"?"payments":"invoices"}/${doc.finance_id}`}>Finance document</Link>}</nav>
        {doc.kind==="opening"&&<p>Balances carried from the previous system. This evidence records no new cash receipt or historical revenue. {snap.openingJournalId&&hasPermission(actor,"finance.read")&&<Link href={`${workspaceBase(publicScope(actor))}/finance/journals/${snap.openingJournalId}`}>Approved opening journal →</Link>}</p>}
        {hasPermission(actor,"finance.read")&&postings.length > 0&&<nav className={styles.sourceLinks} aria-label="Related journals">{postings.map(posting=><Link className={styles.journalLink} key={posting.journal_id} href={`${workspaceBase(publicScope(actor))}/finance/journals/${posting.journal_id}`}>Journal {posting.journal_id.slice(-8)} →</Link>)}</nav>}
        <p className={styles.reference}>PMS-{id}</p>
        <h2>{snap.guest.name}</h2>
        <p>
          {[snap.guest.email, snap.guest.phone].filter(Boolean).join(" · ")}
        </p>
        <p>{snap.folio.name}</p>
        <div className="business-table-wrap">
          <table className={`business-table ${styles.entries}`}>
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
                    <td data-label="Description">{e.data.description}</td>
                    <td data-label="Net">{money(e.data.net)}</td>
                    <td data-label="Service charge">{money(e.data.service)}</td>
                    <td data-label="Tax">{money(e.data.tax)}</td>
                    <td data-label="Total">{money(e.data.total)}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
        <h2>Total {money(snap.amount, snap.property.currency)}</h2>
        <p>Balance due at issue {money(snap.folio.due)}</p>
        <p className="muted">
          Summary document from the PMS ·
          Use your browser&apos;s print command to print or save a PDF
        </p>
      </FeatureWidget>
    </WidgetGrid></FeaturePage>
  );
}
