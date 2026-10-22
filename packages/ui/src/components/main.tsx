"use client";

import { createContext, useContext, type ComponentPropsWithoutRef } from "react";

const MainLandmarkContext = createContext(false);

/** Shells own the page landmark; recovery screens inside them remain sections. */
export function MainContent({ children, ...props }: ComponentPropsWithoutRef<"main">) {
  return (
    <MainLandmarkContext.Provider value={true}>
      <main {...props}>{children}</main>
    </MainLandmarkContext.Provider>
  );
}

export function useMainLandmark() {
  return useContext(MainLandmarkContext);
}
