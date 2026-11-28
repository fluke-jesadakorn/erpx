"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LegalWorkspace = dynamic(() => import("@erpx/legal-ui/components/legal-workspace").then(module => module.LegalWorkspace));
