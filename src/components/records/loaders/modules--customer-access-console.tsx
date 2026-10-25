"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CustomerAccessConsole = dynamic(() => import("@/components/modules/customer-access-console").then(module => module.CustomerAccessConsole));
