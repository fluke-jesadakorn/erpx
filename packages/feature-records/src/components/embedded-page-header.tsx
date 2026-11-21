"use client";
import { useEmbeddedRecord } from "@erpx/ui/components/record-context";
import { PageHeader } from "@erpx/ui/components/primitives";
import type { ReactNode } from "react";

/** The record drawer banner already names the record. Keep the visual page
 * header but avoid a duplicate accessible heading in the drawer. */
export function EmbeddedPageHeader({ eyebrow, title, description, action, appearance = "standard" }: {
  eyebrow?: string;
  title: string;
  description?: string;
  action?: ReactNode;
  appearance?: "standard" | "legacy";
}) {
  return <PageHeader appearance={appearance} eyebrow={eyebrow} title={title} description={description} action={action} decorative={useEmbeddedRecord()} />;
}
