"use client";
import dynamic from "next/dynamic";
export const OrderEditor = dynamic(() => import("@erpx/commerce-ui/components/commerce-controls").then(module => module.OrderEditor));
