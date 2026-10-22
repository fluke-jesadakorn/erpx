"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const RequestActions = dynamic(() => import("@erpx/workplace/components/request-controls").then(module => module.RequestActions));
export const RequestEditor = dynamic(() => import("@erpx/workplace/components/request-controls").then(module => module.RequestEditor));
export const RequestFiles = dynamic(() => import("@erpx/workplace/components/request-controls").then(module => module.RequestFiles));
export const RequestHistory = dynamic(() => import("@erpx/workplace/components/request-controls").then(module => module.RequestHistory));
export const RequestState = dynamic(() => import("@erpx/workplace/components/request-controls").then(module => module.RequestState));
