/* eslint-disable @next/next/no-img-element -- Private brand assets use authenticated routes and must print without optimizer indirection. */
import type { DocumentBrand } from "@erpx/server-brand/public/branding";
import { PrintControls } from "@erpx/feature-brand/components/print-controls";
import styles from "@erpx/feature-brand/components/business-document.module.css";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { businessDocumentBrand } from "@erpx/server-brand/public/business-document";

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
  const brand = await businessDocumentBrand(actor, { kind, recordId, branchId, requestedMode });
  return <BusinessDocumentHeader brand={brand} title={title} reference={reference} printing={printing} />;
}
