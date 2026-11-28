"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupportEvidenceConsole = dynamic(() => import("@erpx/support-ui/components/support-evidence-console").then(module => module.SupportEvidenceConsole));
