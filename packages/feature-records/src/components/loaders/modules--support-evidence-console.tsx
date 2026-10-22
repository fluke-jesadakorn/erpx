"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupportEvidenceConsole = dynamic(() => import("@erpx/feature-support/components/support-evidence-console").then(module => module.SupportEvidenceConsole));
