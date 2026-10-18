import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { parseGuestAccountJson } from "@/lib/identity_access/guest-account-compat";
import { hasPermission } from "@/lib/identity_access/permissions";
import { pmsPresentationQueries } from "@/server/modules/pms/public/presentation-queries";
import {recordCanonicalHref} from "@/lib/records/record-drawer";
import {ActionLink,Card,Table,TableFrame} from "@/components/ui/foundation";
import {PageHeader} from "@/components/ui/primitives";
import { money, type Entry, type GuestAccount, type Guest, type PmsRecord } from "@/server/modules/pms/public/rules/pms-model";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";

import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";



import { RecordLink as Link } from "@/components/records/record-link";
import { notFound } from "next/navigation";
import styles from "@/components/pms/standard-pms-document.module.css";
export const dynamic = "force-dynamic";
export default async function StandardPmsDocument({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    presentation = await pmsPresentationQueries(actor.holdingId);
  requirePermission(actor, hasPermission(actor,"pms.read") ? "pms.read" : "finance.read");
  if (!hasPermission(actor, "pms.cashier") && !hasPermission(actor, "pms.reports")) notFound();
  const queries = presentation.pmsDocumentQueries();
  const doc = await queries.document(id);
  if (!doc) notFound();
  const s = await presentation.loadPms(actor, doc.property_id);
  const postings = await queries.relatedPostings(doc.property_id, id);
  const snap = parseGuestAccountJson(doc.snapshot_json) as {
    property: { name: string; currency: string };
    guest: Guest;
    guestAccount: GuestAccount;
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
  const base=workspaceBase(publicScope(actor));
  return <article className={styles.document} data-pms-document-id={id} data-pms-document-kind={doc.kind}>
    <PageHeader appearance="standard" eyebrow={snap.property.name} title={titles[doc.kind]} description={`${brand.companyName} · ${snap.businessDate}`} action={<ActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/pms/guest-accounts?property=${s.property.id}`)}>Back to bill</ActionLink>}/>
    <Card appearance="standard" className={styles.sheet}>
      <header className={styles.identity}><div><p className={styles.reference}>PMS-{id}</p><h2>{snap.guest.name}</h2><p>{[snap.guest.email,snap.guest.phone].filter(Boolean).join(" · ")}</p><p>{snap.guestAccount.name}</p></div><dl className={styles.summary}><div><dt>Total</dt><dd>{money(snap.amount,snap.property.currency)}</dd></div><div><dt>Balance due at issue</dt><dd>{money(snap.guestAccount.due)}</dd></div></dl></header>
      <nav className={styles.sources} aria-label="Document sources">
        <ActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/pms/guest-accounts?property=${s.property.id}&guestAccount=${doc.guest_account_id}`)}>Guest account</ActionLink>
        {doc.finance_id&&hasPermission(actor,"finance.read")&&<ActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/finance/${["receipt","refund"].includes(doc.kind)||(doc.kind==="opening"&&snap.entries[0]?.data.kind!=="charge")?"payments":"invoices"}/${doc.finance_id}`)}>Finance document</ActionLink>}
      </nav>
      {doc.kind==="opening"&&<p className={styles.note}>Balances carried from the previous system. This evidence records no new cash receipt or historical revenue.{" "}{snap.openingJournalId&&hasPermission(actor,"finance.read")&&<Link href={canonicalUiHref(`${base}/finance/journals/${snap.openingJournalId}`)}>Approved opening journal →</Link>}</p>}
      {hasPermission(actor,"finance.read")&&postings.length>0&&<nav className={styles.sources} aria-label="Related journals">{postings.map(posting=><ActionLink appearance="standard" variant="secondary" key={posting.journal_id} href={canonicalUiHref(`${base}/finance/journals/${posting.journal_id}`)}>Journal {posting.journal_id.slice(-8)} →</ActionLink>)}</nav>}
      <TableFrame appearance="standard" label="Issued document entries" className={styles.table}><Table><thead><tr><th scope="col">Date</th><th scope="col">Description</th><th scope="col">Net</th><th scope="col">Service charge</th><th scope="col">Tax</th><th scope="col">Total</th></tr></thead><tbody>{snap.entries.filter(entry=>doc.kind!=="invoice"||bill(entry.data)).map(entry=><tr key={entry.id}><td>{entry.data.date}</td><td><Link href={recordCanonicalHref(base,{kind:"pms_snapshot_entry",id:entry.id,args:{documentId:id}})}>{entry.data.description}</Link></td><td className={styles.amount}>{money(entry.data.net)}</td><td className={styles.amount}>{money(entry.data.service)}</td><td className={styles.amount}>{money(entry.data.tax)}</td><td className={styles.amount}>{money(entry.data.total)}</td></tr>)}</tbody></Table></TableFrame>
      <p className={styles.print}>Summary document from the PMS · Use your browser&apos;s print command to print or save a PDF</p>
    </Card>
  </article>;
}
