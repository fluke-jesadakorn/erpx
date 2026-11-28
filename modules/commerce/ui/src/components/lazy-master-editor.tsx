"use client";
import dynamic from "next/dynamic";
export const MasterEditor = dynamic(() => import("@erpx/commerce-ui/components/commerce-controls").then(module => module.MasterEditor));
