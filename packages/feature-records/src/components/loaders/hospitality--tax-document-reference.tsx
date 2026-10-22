"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const TaxDocumentReference = dynamic(() => import("@erpx/feature-shared/components/hospitality/tax-document-reference").then(module => module.TaxDocumentReference));
