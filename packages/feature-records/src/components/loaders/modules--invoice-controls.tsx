"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const InvoiceActions = dynamic(() => import("@erpx/feature-finance/components/invoice-controls").then(module => module.InvoiceActions));
export const InvoiceEditor = dynamic(() => import("@erpx/feature-finance/components/invoice-controls").then(module => module.InvoiceEditor));
