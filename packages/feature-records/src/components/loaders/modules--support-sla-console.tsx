"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupportSlaConsole = dynamic(() => import("@erpx/feature-support/components/support-sla-console").then(module => module.SupportSlaConsole));
