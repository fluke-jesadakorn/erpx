"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PrepareControlledPayment = dynamic(() => import("@/components/modules/payment-lifecycle").then(module => module.PrepareControlledPayment));
export const PaymentLifecycle = dynamic(() => import("@/components/modules/payment-lifecycle").then(module => module.PaymentLifecycle));
