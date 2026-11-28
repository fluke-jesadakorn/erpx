"use client";
import dynamic from "next/dynamic";
export const InvoiceEditor = dynamic(() => import("@erpx/finance-ui/components/invoice-controls").then(module => module.InvoiceEditor));
