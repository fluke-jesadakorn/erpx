"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CustomerDuplicateConsole = dynamic(() => import("@erpx/customer-360-ui/components/customer-duplicate-console").then(module => module.CustomerDuplicateConsole));
