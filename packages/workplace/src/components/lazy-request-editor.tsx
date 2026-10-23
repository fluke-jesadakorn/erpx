"use client";
import dynamic from "next/dynamic";
export const RequestEditor = dynamic(() => import("@erpx/workplace/components/request-controls").then(module => module.RequestEditor));
