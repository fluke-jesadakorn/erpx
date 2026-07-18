import type { NextConfig } from "next";
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
      { source: '/all-approvals',   destination: '/inbox?scope=all',     permanent: true },
      { source: '/my-prs',          destination: '/inbox?scope=watching',    permanent: true },
      { source: '/po',              destination: '/inbox?scope=all',     permanent: true },
      { source: '/expense/:id',     destination: '/waybill/by-expense/:id',    permanent: true },
      { source: '/pr/:id',          destination: '/waybill/by-pr/:id',         permanent: true },
      { source: '/po/:id',          destination: '/waybill/by-po/:id',         permanent: true },
    ];
  },
  serverExternalPackages: ["lib/native/vision-ocr"],
  outputFileTracingExcludes: {
    "*": ["./lib/native/vision-ocr/**", "./lib/slips/ocr_lib/**"],
  },
};

export default withNextIntl(nextConfig);
