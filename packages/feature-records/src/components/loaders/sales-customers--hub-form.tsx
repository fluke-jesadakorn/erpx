"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const HubForm = dynamic(() => import("@erpx/feature-sales/components/customers/hub-form").then(module => module.HubForm));
