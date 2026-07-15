import type { NextConfig } from "next";
import path from "node:path";

const LIB_DIR = path.resolve(__dirname, "../lib");

const nextConfig: NextConfig = {
  allowedDevOrigins: ["localhost", "127.0.0.1", "100.97.9.56"],
  typescript: {
    ignoreBuildErrors: true,
  },
  async redirects() {
    return [
      { source: '/dashboard',       destination: '/',                         permanent: true },
      { source: '/submit-expense',  destination: '/expense',                   permanent: true },
      { source: '/expense-claim',   destination: '/expense',                   permanent: true },
      { source: '/approve-expense', destination: '/expense?tab=approve',       permanent: true },
      { source: '/all-approvals',   destination: '/my-waybills?scope=all',     permanent: true },
      { source: '/my-prs',          destination: '/my-waybills?scope=mine',    permanent: true },
      { source: '/po',              destination: '/my-waybills?scope=all',     permanent: true },
      { source: '/expense/:id',     destination: '/waybill/by-expense/:id',    permanent: true },
      { source: '/pr/:id',          destination: '/waybill/by-pr/:id',         permanent: true },
      { source: '/po/:id',          destination: '/waybill/by-po/:id',         permanent: true },
    ];
  },
  serverExternalPackages: ["@folio-lib/native/vision-ocr", "../lib/native/vision-ocr", "lib/native/vision-ocr", "native/vision-ocr"],
  turbopack: {
    resolveAlias: {
      '@folio-lib': LIB_DIR,
      '@folio-lib/native/vision-ocr': path.resolve(LIB_DIR, 'native/vision-ocr'),
    },
  },
};

export default nextConfig;