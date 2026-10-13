"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CRMConsole = dynamic(() => import("@/components/crm/crm-console").then(module => module.CRMConsole));
