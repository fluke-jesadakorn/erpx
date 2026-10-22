"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CRMConsole = dynamic(() => import("@erpx/feature-crm/components/crm-console").then(module => module.CRMConsole));
