"use client";
import dynamic from "next/dynamic";
export const PeopleEditor = dynamic(() => import("@erpx/people-payroll-ui/components/people-editor").then(module => module.PeopleEditor));
