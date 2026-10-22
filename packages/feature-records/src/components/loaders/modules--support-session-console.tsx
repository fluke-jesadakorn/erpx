"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupportSessionConsole = dynamic(() => import("@erpx/feature-support/components/support-session-console").then(module => module.SupportSessionConsole));
