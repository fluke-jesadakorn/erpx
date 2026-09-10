import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft, ArrowUpRight } from "lucide-react";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { getPayment } from "@/server/finance";
import { workspaceBase } from "@/lib/workspace";
import { minorToDecimal } from "@/lib/finance-money";
import { StatusBadge } from "@/components/ui/primitives";

export const dynamic = "force-dynamic";

export default async function PaymentDetail({ params }: { params: Promise<{ holdingSlug: string; id: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  let data;
  try { data = await getPayment(actor, id); } catch { notFound(); }
  const base = workspaceBase(publicScope(actor));
  const payment = data.payment;
  return (
    <main className="content-stack" data-source="live" data-module="finance">
      <Link className="back-link" href={`${base}/finance`}><ArrowLeft size={14} /> Back to finance</Link>
      <div className="section-head">
        <div><p className="eyebrow">Finance · payment detail</p><h1>{String(payment.payment_number)}</h1><p className="muted">{String(payment.party_name ?? "Unassigned party")} · {String(payment.kind)} · {String(payment.currency)}</p></div>
        <StatusBadge status={String(payment.status)} />
      </div>
      <section className="card-grid" aria-label="Payment summary">
        {[["Payment date", payment.payment_date], ["Bank account", payment.bank_account ?? "Not assigned"], ["Amount", `${payment.currency} ${minorToDecimal(Number(payment.amount_minor))}`], ["Outstanding", `${payment.currency} ${minorToDecimal(Number(payment.outstanding_minor))}`]].map(([label, value]) => <article className="card" key={String(label)}><span className="eyebrow">{String(label)}</span><strong>{String(value)}</strong></article>)}
      </section>
      <section className="card">
        <div className="section-head"><h2>Invoice allocations</h2><span className="badge">{data.allocations.length}</span></div>
        {data.allocations.length === 0 ? <p className="muted">This payment has not been allocated yet.</p> : data.allocations.map((allocation) => <div className="row" key={String(allocation.id)}><div className="row-main"><strong className="row-title">{String(allocation.invoice_number)}</strong><span className="muted">{String(allocation.status)} · {String(allocation.created_at)}</span></div><strong>{String(payment.currency)} {minorToDecimal(Number(allocation.amount_minor))}<ArrowUpRight size={14} /></strong></div>)}
      </section>
    </main>
  );
}
