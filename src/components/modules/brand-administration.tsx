"use client";

import { useState } from "react";
import Image from "next/image";
import type {
  DocumentBrandMode,
  StorefrontAppBrand,
  WorkspaceBrand,
} from "@/server/branding";
import { workFetch } from "@/lib/work-fetch";
import { WorkspaceBrandMark } from "@/components/brand/workspace-brand";
import { MediaPublicationPanel } from "@/components/attachments/media-publication-panel";
import {
  canMutateAttachments,
  useAttachmentFeatureMode,
} from "@/components/attachments/attachment-feature-context";
import styles from "./brand-administration.module.css";

type Props = {
  base: string;
  holding: WorkspaceBrand;
  branches: WorkspaceBrand[];
  storefront: StorefrontAppBrand;
  canHolding: boolean;
  canBranches: boolean;
  canStorefront: boolean;
  canReviewPublications: boolean;
};

export function BrandAdministration({
  base,
  holding,
  branches,
  storefront,
  canHolding,
  canBranches,
  canStorefront,
  canReviewPublications,
}: Props) {
  const attachmentMode = useAttachmentFeatureMode();
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");
  const [shortName, setShortName] = useState(storefront.shortName);
  const [accentColor, setAccentColor] = useState(storefront.accentColor);
  const endpoint = `/api/v1${base}`;

  async function upload(
    ownerType: "holding_brand" | "branch_brand" | "company_storefront_brand",
    ownerId: string,
    variant: "primary" | "inverse" | "logo",
    file: File,
  ) {
    if (attachmentMode !== "enabled") return;
    setBusy(`${ownerType}:${ownerId}:${variant}`);
    setMessage("");
    try {
      const reserve = await workFetch(`${endpoint}/uploads`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": crypto.randomUUID(),
        },
        body: JSON.stringify({
          ownerType,
          ownerId,
          purpose: variant,
          filename: file.name,
          mediaType: file.type,
          bytes: file.size,
        }),
      });
      const reserved = (await reserve.json()) as {
        data?: { uploadUrl: string };
        error?: { message: string };
      };
      if (!reserve.ok || !reserved.data)
        throw new Error(
          reserved.error?.message ?? "Logo upload could not start.",
        );
      const stored = await workFetch(reserved.data.uploadUrl, {
        method: "PUT",
        headers: { "Content-Type": file.type },
        body: file,
      });
      const result = (await stored.json()) as { error?: { message: string } };
      if (!stored.ok)
        throw new Error(result.error?.message ?? "Logo upload failed.");
      window.location.reload();
    } catch (error) {
      setMessage(
        error instanceof Error ? error.message : "Logo upload failed.",
      );
      setBusy("");
    }
  }

  async function patch(
    path: string,
    body: {
      documentMode?: DocumentBrandMode;
      remove?: "primary" | "inverse";
      shortName?: string;
      accentColor?: string;
      removeLogo?: boolean;
    },
  ) {
    if (
      attachmentMode !== "enabled" &&
      (body.remove !== undefined || body.removeLogo === true)
    )
      return;
    setBusy(path + JSON.stringify(body));
    setMessage("");
    const response = await workFetch(`${endpoint}/branding/${path}`, {
      method: "PATCH",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": crypto.randomUUID(),
      },
      body: JSON.stringify(body),
    });
    const result = (await response.json()) as { error?: { message: string } };
    if (!response.ok) {
      setMessage(result.error?.message ?? "Branding could not be updated.");
      setBusy("");
      return;
    }
    window.location.reload();
  }

  function editor(
    brand: WorkspaceBrand,
    ownerType: "holding_brand" | "branch_brand",
    canEdit: boolean,
  ) {
    const path =
      brand.scopeKind === "holding"
        ? "holding"
        : `branches/${encodeURIComponent(brand.scopeId)}`;
    const canMutateFiles = canMutateAttachments(attachmentMode, canEdit);
    return (
      <article
        className={styles.profile}
        key={`${brand.scopeKind}:${brand.scopeId}`}
      >
        <div className={styles.preview}>
          <WorkspaceBrandMark brand={brand} />
          <div>
            <strong>{brand.label}</strong>
            <small>
              {brand.scopeKind === "holding" ? "Holding brand" : "Branch brand"}
            </small>
          </div>
        </div>
        {canEdit && (canMutateFiles || brand.scopeKind === "branch") && (
          <div className={styles.actions}>
            {canMutateFiles &&
              (["primary", "inverse"] as const).map((variant) => (
                <label className="button secondary" key={variant}>
                  {busy === `${ownerType}:${brand.scopeId}:${variant}`
                    ? "Uploading…"
                    : `Upload ${variant}`}
                  <input
                    hidden
                    type="file"
                    accept="image/png,image/webp"
                    disabled={!!busy}
                    onChange={(event) => {
                      const file = event.target.files?.[0];
                      if (file)
                        void upload(ownerType, brand.scopeId, variant, file);
                    }}
                  />
                </label>
              ))}
            {canMutateFiles && brand.primaryUrl && (
              <button
                className="button secondary"
                disabled={!!busy}
                onClick={() => void patch(path, { remove: "primary" })}
              >
                Remove primary
              </button>
            )}
            {canMutateFiles && brand.inverseUrl && (
              <button
                className="button secondary"
                disabled={!!busy}
                onClick={() => void patch(path, { remove: "inverse" })}
              >
                Remove inverse
              </button>
            )}
            {brand.scopeKind === "branch" && (
              <label>
                Document default
                <select
                  value={brand.documentMode}
                  disabled={!!busy}
                  onChange={(event) =>
                    void patch(path, {
                      documentMode: event.target.value as DocumentBrandMode,
                    })
                  }
                >
                  <option value="branch">Branch only</option>
                  <option value="holding">Holding only</option>
                  <option value="both">Holding + Branch</option>
                </select>
              </label>
            )}
          </div>
        )}
        {(["primary", "inverse"] as const).map((variant) => (
          <MediaPublicationPanel
            key={variant}
            base={base}
            targetType={ownerType}
            targetId={brand.scopeId}
            purposeKey={variant}
            sourceComponentKind="publication"
            title={`${brand.label} · ${variant} approved media`}
            canCreate={canEdit}
            canReview={canEdit && canReviewPublications}
          />
        ))}
      </article>
    );
  }

  return (
    <section className="card">
      <h2>Workspace branding</h2>
      <p className="muted">
        ERPX remains the product brand. These private PNG or WebP assets
        identify the Holding and Branch in the workspace and printed documents.
      </p>
      {message && (
        <p className="error" role="alert">
          {message}
        </p>
      )}
      <article className={styles.profile}>
        <div className={styles.preview}>
          <span
            style={{
              width: 52,
              height: 52,
              display: "grid",
              placeItems: "center",
              overflow: "hidden",
              borderRadius: 14,
              background: storefront.accentColor,
              color: "#fff",
              fontWeight: 700,
            }}
          >
            {storefront.logoUrl ? (
              <Image
                src={storefront.logoUrl}
                alt=""
                width={52}
                height={52}
                unoptimized
                style={{ width: "100%", height: "100%", objectFit: "contain" }}
              />
            ) : (
              (Array.from(shortName.trim())[0]?.toLocaleUpperCase() ?? "•")
            )}
          </span>
          <div>
            <strong>
              {shortName} POS · {shortName} PMS
            </strong>
            <small>
              Company storefront app brand · legal issuer stays{" "}
              {storefront.companyName}
            </small>
          </div>
        </div>
        {canStorefront && (
          <div className={styles.actions}>
            <label>
              Short app name
              <input
                value={shortName}
                maxLength={40}
                onChange={(event) => setShortName(event.target.value)}
              />
            </label>
            <label>
              App color
              <input
                type="color"
                value={accentColor}
                onChange={(event) => setAccentColor(event.target.value)}
              />
            </label>
            <button
              className="button"
              disabled={!!busy || !shortName.trim()}
              onClick={() =>
                void patch("storefront", {
                  shortName: shortName.trim(),
                  accentColor,
                })
              }
            >
              Save app brand
            </button>
            {attachmentMode === "enabled" && (
              <label className="button secondary">
                {busy ===
                "company_storefront_brand:" + storefront.companyId + ":logo"
                  ? "Uploading…"
                  : "Upload app logo"}
                <input
                  hidden
                  type="file"
                  accept="image/png,image/webp"
                  disabled={!!busy}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (file)
                      void upload(
                        "company_storefront_brand",
                        storefront.companyId,
                        "logo",
                        file,
                      );
                  }}
                />
              </label>
            )}
            {attachmentMode === "enabled" && storefront.logoUrl && (
              <button
                className="button secondary"
                disabled={!!busy}
                onClick={() => void patch("storefront", { removeLogo: true })}
              >
                Remove app logo
              </button>
            )}
            <span>
              <a
                href={
                  "/front/" +
                  encodeURIComponent(storefront.holdingSlug) +
                  "/" +
                  encodeURIComponent(storefront.companySlug) +
                  "/pos/"
                }
              >
                Install POS
              </a>
              {" · "}
              <a
                href={
                  "/front/" +
                  encodeURIComponent(storefront.holdingSlug) +
                  "/" +
                  encodeURIComponent(storefront.companySlug) +
                  "/pms/"
                }
              >
                Install PMS
              </a>
            </span>
          </div>
        )}
        <MediaPublicationPanel
          base={base}
          targetType="company_storefront_brand"
          targetId={storefront.companyId}
          purposeKey="logo"
          sourceComponentKind="publication"
          title={`${storefront.shortName} · approved app logo`}
          canCreate={canStorefront}
          canReview={canStorefront && canReviewPublications}
        />
      </article>
      <div className={styles.grid}>
        {editor(holding, "holding_brand", canHolding)}
        {branches.map((branch) => editor(branch, "branch_brand", canBranches))}
      </div>
    </section>
  );
}
