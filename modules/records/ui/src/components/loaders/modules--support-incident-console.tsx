"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const SupportIncidentConsole = dynamic(() => import("@erpx/support-ui/components/support-incident-console").then(module => module.SupportIncidentConsole));
