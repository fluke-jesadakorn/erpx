"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const BrandIdentityConsole = dynamic(() => import("@erpx/feature-brand/components/brand-identity-console").then(module => module.BrandIdentityConsole));
