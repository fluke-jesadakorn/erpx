"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PaymentCorrection = dynamic(() => import("@/components/finance/payment-correction").then(module => module.PaymentCorrection));
