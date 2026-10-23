"use client";
import dynamic from "next/dynamic";
export const PeopleEditor = dynamic(() => import("@erpx/feature-people_payroll/components/people-editor").then(module => module.PeopleEditor));
