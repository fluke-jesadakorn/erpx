"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const VatCorrectionPreparation = dynamic(() => import("@/components/finance/vat-register-controls").then(module => module.VatCorrectionPreparation));
export const VatCorrectionReview = dynamic(() => import("@/components/finance/vat-register-controls").then(module => module.VatCorrectionReview));
export const VatPackageControls = dynamic(() => import("@/components/finance/vat-register-controls").then(module => module.VatPackageControls));
