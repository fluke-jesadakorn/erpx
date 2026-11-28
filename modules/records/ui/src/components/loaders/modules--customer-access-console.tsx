"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CustomerAccessConsole = dynamic(() => import("@erpx/customer-360-ui/components/customer-access-console").then(module => module.CustomerAccessConsole));
