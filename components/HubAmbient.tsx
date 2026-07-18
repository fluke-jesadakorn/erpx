import React from 'react';

interface HubAmbientProps {
  className?: string;
  variant?: 'hero' | 'page';
}

export const HubAmbient: React.FC<HubAmbientProps> = ({ className = '', variant = 'hero' }) => {
  return (
    <div
      aria-hidden="true"
      className={`absolute inset-0 overflow-hidden pointer-events-none ${className}`}
    >
      <div
        className="absolute inset-0"
        style={{
          backgroundImage:
            'linear-gradient(color-mix(in oklab, var(--rule) 32%, transparent) 1px, transparent 1px), linear-gradient(90deg, color-mix(in oklab, var(--rule) 32%, transparent) 1px, transparent 1px)',
          backgroundSize: '32px 32px',
          opacity: variant === 'hero' ? 0.5 : 0.35,
          maskImage:
            'radial-gradient(ellipse at 50% 0%, rgba(0,0,0,0.9) 0%, rgba(0,0,0,0.4) 60%, rgba(0,0,0,0) 100%)',
          WebkitMaskImage:
            'radial-gradient(ellipse at 50% 0%, rgba(0,0,0,0.9) 0%, rgba(0,0,0,0.4) 60%, rgba(0,0,0,0) 100%)',
        }}
      />

      <div
        className="absolute -left-24 -top-24 h-[28rem] w-[28rem] rounded-full bg-accent-soft/45 blur-3xl animate-float-slow"
      />
      <div
        className="absolute -right-20 -top-32 h-[26rem] w-[26rem] rounded-full bg-info-soft/35 blur-3xl animate-float-slower"
      />
      <div
        className="absolute -bottom-28 left-1/3 h-[24rem] w-[24rem] rounded-full bg-positive-soft/25 blur-3xl animate-float-slow"
      />

      {variant === 'hero' && (
        <div className="absolute inset-x-0 top-0 h-px overflow-hidden">
          <div className="h-full w-1/3 bg-gradient-to-r from-transparent via-accent/70 to-transparent animate-hero-scan" />
        </div>
      )}
    </div>
  );
};

export default HubAmbient;
