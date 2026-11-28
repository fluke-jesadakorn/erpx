"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CRMActivityConsole = dynamic(() => import("@erpx/crm-ui/components/crm-activity-console").then(module => module.CRMActivityConsole));
