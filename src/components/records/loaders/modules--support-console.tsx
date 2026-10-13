"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupportConsole = dynamic(() => import("@/components/support/support-console").then(module => module.SupportConsole));
