"use client";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import Image from "next/image";
import type { DocumentBrandMode, StorefrontAppBrand, WorkspaceBrand } from "@/server/branding";
import { workFetch } from "@/lib/work-fetch";
import { WorkspaceBrandMark } from "@/components/brand/workspace-brand";
import { MediaPublicationPanel } from "@/components/attachments/media-publication-panel";
import {
  canMutateAttachments,
  useAttachmentFeatureMode,
} from "@/components/attachments/attachment-feature-context";
import styles from "./brand-administration.module.css";
import { type UiAppearance } from "@/components/ui/foundation";

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

export function BrandAdministration({ appearance = "legacy",
  base,
  holding,
  branches,
  storefront,
  canHolding,
  canBranches,
  canStorefront,
  canReviewPublications,
}: (Props) & { appearance?: UiAppearance }) {
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
        throw new Error(reserved.error?.message ?? "Logo upload could not start.");
      const stored = await workFetch(reserved.data.uploadUrl, {
        method: "PUT",
        headers: { "Content-Type": file.type },
        body: file,
      });
      const result = (await stored.json()) as { error?: { message: string } };
      if (!stored.ok) throw new Error(result.error?.message ?? "Logo upload failed.");
      window.location.reload();
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Logo upload failed.");
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
    if (attachmentMode !== "enabled" && (body.remove !== undefined || body.removeLogo === true))
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
      brand.scopeKind === "holding" ? "holding" : `branches/${encodeURIComponent(brand.scopeId)}`;
    const canMutateFiles = canMutateAttachments(attachmentMode, canEdit);
    return (
      <article data-ui-appearance={appearance} className={styles.profile} key={`${brand.scopeKind}:${brand.scopeId}`}>
        <div data-ui-appearance={appearance} className={styles.preview}>
          <WorkspaceBrandMark appearance={appearance} brand={brand} />
          <div data-ui-appearance={appearance}>
            <strong data-ui-appearance={appearance}>{brand.label}</strong>
            <small data-ui-appearance={appearance}>{brand.scopeKind === "holding" ? "Holding brand" : "Branch brand"}</small>
          </div>
        </div>
        {canEdit && (canMutateFiles || brand.scopeKind === "branch") && (
          <div data-ui-appearance={appearance} className={styles.actions}>
            {canMutateFiles &&
              (["primary", "inverse"] as const).map((variant) => (
                <Field appearance={appearance} className="button secondary" key={variant}>
                  {busy === `${ownerType}:${brand.scopeId}:${variant}`
                    ? "Uploading…"
                    : `Upload ${variant}`}
                  <TextInput appearance={appearance}
                    hidden
                    type="file"
                    accept="image/png,image/webp"
                    disabled={!!busy}
                    onChange={(event) => {
                      const file = event.target.files?.[0];
                      if (file) void upload(ownerType, brand.scopeId, variant, file);
                    }}
                  />
                </Field>
              ))}
            {canMutateFiles && brand.primaryUrl && (
              <Button appearance={appearance}
                type="submit"
                variant="secondary"

                disabled={!!busy}
                onClick={() => void patch(path, { remove: "primary" })}
              >
                Remove primary
              </Button>
            )}
            {canMutateFiles && brand.inverseUrl && (
              <Button appearance={appearance}
                type="submit"
                variant="secondary"

                disabled={!!busy}
                onClick={() => void patch(path, { remove: "inverse" })}
              >
                Remove inverse
              </Button>
            )}
            {brand.scopeKind === "branch" && (
              <Field appearance={appearance}>
                Document default
                <Select appearance={appearance}
                  value={brand.documentMode}
                  disabled={!!busy}
                  onChange={(event) =>
                    void patch(path, {
                      documentMode: event.target.value as DocumentBrandMode,
                    })
                  }
                >
                  <option data-ui-appearance={appearance} value="branch">Branch only</option>
                  <option data-ui-appearance={appearance} value="holding">Holding only</option>
                  <option data-ui-appearance={appearance} value="both">Holding + Branch</option>
                </Select>
              </Field>
            )}
          </div>
        )}
        {(["primary", "inverse"] as const).map((variant) => (
          <MediaPublicationPanel appearance={appearance}
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
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <h2 data-ui-appearance={appearance}>Workspace branding</h2>
      <p data-ui-appearance={appearance} className="muted">
        ERPX remains the product brand. These private PNG or WebP assets identify the Holding and
        Branch in the workspace and printed documents.
      </p>
      {message && (
        <p data-ui-appearance={appearance} className="error" role="alert">
          {message}
        </p>
      )}
      <article data-ui-appearance={appearance} className={styles.profile}>
        <div data-ui-appearance={appearance} className={styles.preview}>
          <span data-ui-appearance={appearance}
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
          <div data-ui-appearance={appearance}>
            <strong data-ui-appearance={appearance}>
              {shortName} POS · {shortName} PMS
            </strong>
            <small data-ui-appearance={appearance}>
              Company storefront app brand · legal issuer stays {storefront.companyName}
            </small>
          </div>
        </div>
        {canStorefront && (
          <div data-ui-appearance={appearance} className={styles.actions}>
            <Field appearance={appearance}>
              Short app name
              <TextInput appearance={appearance}
                value={shortName}
                maxLength={40}
                onChange={(event) => setShortName(event.target.value)}
              />
            </Field>
            <Field appearance={appearance}>
              App color
              <TextInput appearance={appearance}
                type="color"
                value={accentColor}
                onChange={(event) => setAccentColor(event.target.value)}
              />
            </Field>
            <Button appearance={appearance}
              type="submit"
              variant="primary"

              disabled={!!busy || !shortName.trim()}
              onClick={() =>
                void patch("storefront", {
                  shortName: shortName.trim(),
                  accentColor,
                })
              }
            >
              Save app brand
            </Button>
            {attachmentMode === "enabled" && (
              <Field appearance={appearance} className="button secondary">
                {busy === "company_storefront_brand:" + storefront.companyId + ":logo"
                  ? "Uploading…"
                  : "Upload app logo"}
                <TextInput appearance={appearance}
                  hidden
                  type="file"
                  accept="image/png,image/webp"
                  disabled={!!busy}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (file)
                      void upload("company_storefront_brand", storefront.companyId, "logo", file);
                  }}
                />
              </Field>
            )}
            {attachmentMode === "enabled" && storefront.logoUrl && (
              <Button appearance={appearance}
                type="submit"
                variant="secondary"

                disabled={!!busy}
                onClick={() => void patch("storefront", { removeLogo: true })}
              >
                Remove app logo
              </Button>
            )}
            <span data-ui-appearance={appearance}>
              <a data-ui-appearance={appearance}
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
              <a data-ui-appearance={appearance}
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
        <MediaPublicationPanel appearance={appearance}
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
      <div data-ui-appearance={appearance} className={styles.grid}>
        {editor(holding, "holding_brand", canHolding)}
        {branches.map((branch) => editor(branch, "branch_brand", canBranches))}
      </div>
    </section>
  );
}
