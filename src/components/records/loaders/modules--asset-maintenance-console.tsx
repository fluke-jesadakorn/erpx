"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const AssetConsole = dynamic(() => import("@/components/assets/asset-maintenance-console").then(module => module.AssetConsole));
export const MaintenanceConsole = dynamic(() => import("@/components/assets/asset-maintenance-console").then(module => module.MaintenanceConsole));
