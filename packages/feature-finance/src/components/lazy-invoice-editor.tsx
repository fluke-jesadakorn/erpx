"use client";
import dynamic from "next/dynamic";
export const InvoiceEditor = dynamic(() => import("@erpx/feature-finance/components/invoice-controls").then(module => module.InvoiceEditor));
