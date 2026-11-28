"use client";
import dynamic from "next/dynamic";
export const TransferEditor = dynamic(() => import("@erpx/inventory-ui/components/stock-transfer-controls").then(module => module.TransferEditor));
