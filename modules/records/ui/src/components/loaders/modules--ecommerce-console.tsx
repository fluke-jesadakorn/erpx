"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const EcommerceConsole = dynamic(() => import("@erpx/ecommerce-ui/components/ecommerce-console").then(module => module.EcommerceConsole));
