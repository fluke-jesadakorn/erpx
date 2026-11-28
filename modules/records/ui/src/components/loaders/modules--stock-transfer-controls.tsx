"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const TransferActions = dynamic(() => import("@erpx/inventory-ui/components/stock-transfer-controls").then(module => module.TransferActions));
export const TransferEditor = dynamic(() => import("@erpx/inventory-ui/components/stock-transfer-controls").then(module => module.TransferEditor));
