"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const InventoryReorderConsole = dynamic(() => import("@erpx/feature-inventory/components/inventory-reorder-console").then(module => module.InventoryReorderConsole));
