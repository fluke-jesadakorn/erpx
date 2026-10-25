"use client";

import dynamic from "next/dynamic";

// The authorized Server recovery panel renders this only for platform recovery.
// Keep SSR while deferring the picker code on ordinary workspace pages.
export const PlatformRoleSwitcher = dynamic(() =>
  import("./platform-role-switcher").then(module => module.PlatformRoleSwitcher),
);
