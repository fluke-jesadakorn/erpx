"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PeopleEditor = dynamic(() => import("@/components/people_payroll/people-editor").then(module => module.PeopleEditor));
