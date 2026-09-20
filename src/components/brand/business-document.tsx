/* eslint-disable @next/next/no-img-element -- Private brand assets use authenticated routes and must print without optimizer indirection. */
import type { DocumentBrand } from "@/server/branding";
import { PrintControls } from "./print-controls";
import styles from "./business-document.module.css";
import type { WorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { recordBranchId, resolveDocumentBrand } from "@/server/branding";

function Mark({ brand }: { brand: DocumentBrand["holding"] }) {
  return <div className={styles.identity}>{brand.primaryUrl ? <img src={brand.primaryUrl} alt="" /> : <span aria-hidden="true">{brand.initials}</span>}<strong>{brand.label}</strong></div>;
}

export function BusinessDocumentHeader({ brand, title, reference, printing }: { brand: DocumentBrand; title: string; reference?: string; printing: boolean }) {
  return <>
    <PrintControls defaultMode={brand.mode} hasBranch={!!brand.branch} printing={printing} />
    {printing && <header className={styles.header}>
      <div className={styles.brands}>
        {(brand.mode === "holding" || brand.mode === "both") && <Mark brand={brand.holding} />}
        {(brand.mode === "branch" || brand.mode === "both") && brand.branch && <Mark brand={brand.branch} />}
      </div>
      {brand.mode === "both" && brand.branch && <p className={styles.endorsement}>{brand.branch.label} · Part of {brand.holding.label}</p>}
      <div className={styles.legal}><span>Legal issuer</span><strong>{brand.companyName}</strong>{brand.branchName && <small>Operating unit · {brand.branchName}</small>}</div>
      <div className={styles.document}><span>{title}</span>{reference && <strong>{reference}</strong>}</div>
    </header>}
  </>;
}

export async function BusinessDocumentBranding({
  actor,
  kind,
  recordId,
  branchId,
  requestedMode,
  title,
  reference,
  printing,
}: {
  actor: WorkContext;
  kind?: "order" | "invoice" | "payment" | "journal" | "expense";
  recordId?: string;
  branchId?: string | null;
  requestedMode?: string | null;
  title: string;
  reference?: string;
  printing: boolean;
}) {
  const { write } = await holdingResources(actor.holdingId);
  const resolvedBranch = kind && recordId ? await recordBranchId(write, actor, kind, recordId) : branchId ?? null;
  const brand = await resolveDocumentBrand(write, actor, { branchId: resolvedBranch, requestedMode });
  return <BusinessDocumentHeader brand={brand} title={title} reference={reference} printing={printing} />;
}
