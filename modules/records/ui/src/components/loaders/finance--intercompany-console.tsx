"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const IntercompanyConsole = dynamic(() => import("@erpx/finance-ui/components/intercompany-console").then(module => module.IntercompanyConsole));
