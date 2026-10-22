"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupportConsole = dynamic(() => import("@erpx/feature-support/components/support-console").then(module => module.SupportConsole));
