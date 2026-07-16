import type { NextConfig } from "next";
import path from "node:path";
import createNextIntlPlugin from 'next-intl/plugin';

const withNextIntl = createNextIntlPlugin('./src/i18n.ts');

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
  serverExternalPackages: ["lib/native/vision-ocr"],
  turbopack: {
    resolveAlias: {
      '@/components': path.resolve(__dirname, "components"),
      '@/app': path.resolve(__dirname, "app"),
      '@/i18n': path.resolve(__dirname, "src/i18n"),
      '@': path.resolve(__dirname, "lib"),
    },
  },
};

export default withNextIntl(nextConfig);
