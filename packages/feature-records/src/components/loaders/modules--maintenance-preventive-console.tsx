"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const MaintenancePreventiveConsole = dynamic(() => import("@erpx/feature-maintenance/components/maintenance-preventive-console").then(module => module.MaintenancePreventiveConsole));
