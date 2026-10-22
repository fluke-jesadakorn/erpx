"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const IntercompanyConsole = dynamic(() => import("@erpx/feature-finance/components/intercompany-console").then(module => module.IntercompanyConsole));
