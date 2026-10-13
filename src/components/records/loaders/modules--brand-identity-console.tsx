"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const BrandIdentityConsole = dynamic(() => import("@/components/brand/brand-identity-console").then(module => module.BrandIdentityConsole));
