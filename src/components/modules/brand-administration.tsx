"use client";

import { useState } from "react";
import type { DocumentBrandMode, WorkspaceBrand } from "@/server/branding";
import { workFetch } from "@/lib/work-fetch";
import { WorkspaceBrandMark } from "@/components/brand/workspace-brand";
import styles from "./brand-administration.module.css";

type Props = {
  base: string;
  holding: WorkspaceBrand;
  branches: WorkspaceBrand[];
  canHolding: boolean;
  canBranches: boolean;
};

export function BrandAdministration({ base, holding, branches, canHolding, canBranches }: Props) {
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");
  const endpoint = `/api/v1${base}`;

  async function upload(ownerType: "holding_brand" | "branch_brand", ownerId: string, variant: "primary" | "inverse", file: File) {
    setBusy(`${ownerType}:${ownerId}:${variant}`);
    setMessage("");
    try {
      const reserve = await workFetch(`${endpoint}/uploads`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ ownerType, ownerId, purpose: variant, filename: file.name, mediaType: file.type, bytes: file.size }),
      });
      const reserved = await reserve.json() as { data?: { uploadUrl: string }; error?: { message: string } };
      if (!reserve.ok || !reserved.data) throw new Error(reserved.error?.message ?? "Logo upload could not start.");
      const stored = await workFetch(reserved.data.uploadUrl, { method: "PUT", headers: { "Content-Type": file.type }, body: file });
      const result = await stored.json() as { error?: { message: string } };
      if (!stored.ok) throw new Error(result.error?.message ?? "Logo upload failed.");
      window.location.reload();
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Logo upload failed.");
      setBusy("");
    }
  }

  async function patch(path: string, body: { documentMode?: DocumentBrandMode; remove?: "primary" | "inverse" }) {
    setBusy(path + JSON.stringify(body));
    setMessage("");
    const response = await workFetch(`${endpoint}/branding/${path}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
      body: JSON.stringify(body),
    });
    const result = await response.json() as { error?: { message: string } };
    if (!response.ok) {
      setMessage(result.error?.message ?? "Branding could not be updated.");
      setBusy("");
      return;
    }
    window.location.reload();
  }

  function editor(brand: WorkspaceBrand, ownerType: "holding_brand" | "branch_brand", canEdit: boolean) {
    const path = brand.scopeKind === "holding" ? "holding" : `branches/${encodeURIComponent(brand.scopeId)}`;
    return (
      <article className={styles.profile} key={`${brand.scopeKind}:${brand.scopeId}`}>
        <div className={styles.preview}><WorkspaceBrandMark brand={brand} /><div><strong>{brand.label}</strong><small>{brand.scopeKind === "holding" ? "Holding brand" : "Branch brand"}</small></div></div>
        {canEdit && <div className={styles.actions}>
          {(["primary", "inverse"] as const).map((variant) => <label className="button secondary" key={variant}>
            {busy === `${ownerType}:${brand.scopeId}:${variant}` ? "Uploading…" : `Upload ${variant}`}
            <input hidden type="file" accept="image/png,image/webp" disabled={!!busy} onChange={(event) => { const file = event.target.files?.[0]; if (file) void upload(ownerType, brand.scopeId, variant, file); }} />
          </label>)}
          {brand.primaryUrl && <button className="button secondary" disabled={!!busy} onClick={() => void patch(path, { remove: "primary" })}>Remove primary</button>}
          {brand.inverseUrl && <button className="button secondary" disabled={!!busy} onClick={() => void patch(path, { remove: "inverse" })}>Remove inverse</button>}
          {brand.scopeKind === "branch" && <label>Document default<select value={brand.documentMode} disabled={!!busy} onChange={(event) => void patch(path, { documentMode: event.target.value as DocumentBrandMode })}><option value="branch">Branch only</option><option value="holding">Holding only</option><option value="both">Holding + Branch</option></select></label>}
        </div>}
      </article>
    );
  }

  return <section className="card">
    <h2>Workspace branding</h2>
    <p className="muted">ERPX remains the product brand. These private PNG or WebP assets identify the Holding and Branch in the workspace and printed documents.</p>
    {message && <p className="error" role="alert">{message}</p>}
    <div className={styles.grid}>{editor(holding, "holding_brand", canHolding)}{branches.map((branch) => editor(branch, "branch_brand", canBranches))}</div>
  </section>;
}
