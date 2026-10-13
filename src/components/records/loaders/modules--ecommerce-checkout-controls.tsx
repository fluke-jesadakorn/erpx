"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const EcommerceCheckoutControls = dynamic(() => import("@/components/ecommerce/ecommerce-checkout-controls").then(module => module.EcommerceCheckoutControls));
