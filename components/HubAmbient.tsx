import React from 'react';

interface HubAmbientProps {
  className?: string;
  variant?: 'hero' | 'page';
}

const DOT_BG = [
  "url(\"data:image/svg+xml;utf8,",
  "<svg xmlns='http://www.w3.org/2000/svg' width='28' height='28' viewBox='0 0 28 28'>",
  "<circle cx='1' cy='1' r='1' fill='%23a5b4fc' fill-opacity='0.35'/>",
  "</svg>",
  "\")",
].join('');

export const HubAmbient: React.FC<HubAmbientProps> = ({ className = '', variant = 'hero' }) => {
  return (
    <div
      aria-hidden="true"
      className={`absolute inset-0 overflow-hidden pointer-events-none ${className}`}
    >
      <div
        className="absolute inset-0"
        style={{
          backgroundImage: DOT_BG,
          backgroundSize: '28px 28px',
          opacity: variant === 'hero' ? 0.5 : 0.35,
          maskImage:
            'radial-gradient(ellipse at 50% 0%, rgba(0,0,0,0.9) 0%, rgba(0,0,0,0.4) 60%, rgba(0,0,0,0) 100%)',
          WebkitMaskImage:
            'radial-gradient(ellipse at 50% 0%, rgba(0,0,0,0.9) 0%, rgba(0,0,0,0.4) 60%, rgba(0,0,0,0) 100%)',
        }}
      />

      <div
        className="absolute -top-24 -left-24 w-[28rem] h-[28rem] rounded-full bg-indigo-500/25 blur-3xl animate-float-slow"
      />
      <div
        className="absolute -top-32 -right-20 w-[26rem] h-[26rem] rounded-full bg-purple-500/25 blur-3xl animate-float-slower"
      />
      <div
        className="absolute -bottom-28 left-1/3 w-[24rem] h-[24rem] rounded-full bg-cyan-500/15 blur-3xl animate-float-slow"
      />

      {variant === 'hero' && (
        <div className="absolute inset-x-0 top-0 h-px overflow-hidden">
          <div className="h-full w-1/3 bg-gradient-to-r from-transparent via-indigo-300/70 to-transparent animate-hero-scan" />
        </div>
      )}
    </div>
  );
};

export default HubAmbient;
