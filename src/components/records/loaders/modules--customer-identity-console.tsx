"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CustomerIdentityConsole = dynamic(() => import("@/components/customer_360/customer-identity-console").then(module => module.CustomerIdentityConsole));
