"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupportSlaConsole = dynamic(() => import("@/components/modules/support-sla-console").then(module => module.SupportSlaConsole));
