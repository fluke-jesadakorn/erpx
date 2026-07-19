'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import { Check, Code2, Copy, Eye, Maximize2, Minimize2 } from 'lucide-react';

type View = 'preview' | 'html' | 'css' | 'js';

interface Artifact {
  title: string;
  html: string;
  css: string;
  js: string;
  full: string;
}

const BASE_CSS = `*{box-sizing:border-box}html{color-scheme:dark}body{margin:0;min-height:100%;background:#071018;color:#e7f0f6;font:14px/1.55 Inter,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}button,input,select{font:inherit}`;
const SAFE_CSS = `:where(img,svg,video,canvas){max-width:100%;height:auto}:where(pre,code){white-space:pre-wrap;overflow-wrap:anywhere}:where(body,body *){overflow-wrap:anywhere}`;
const PRELUDE_JS = `(function(){var sent=false;function notify(event){if(event&&event.preventDefault)event.preventDefault();if(sent)return;sent=true;parent.postMessage({type:'folio-artifact-error'},'*')}console.error=notify;addEventListener('error',notify);addEventListener('unhandledrejection',notify)})();`;
const BOOT_JS = `(function(){
  function color(value){var n=String(value).match(/[\\d.]+/g);if(!n||n.length<3)return null;return [Number(n[0]),Number(n[1]),Number(n[2]),n.length>3?Number(n[3]):1]}
  function lum(rgb){var values=rgb.slice(0,3).map(function(v){v/=255;return v<=.04045?v/12.92:Math.pow((v+.055)/1.055,2.4)});return values[0]*.2126+values[1]*.7152+values[2]*.0722}
  function ratio(a,b){var x=lum(a),y=lum(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05)}
  function background(el){for(var node=el;node&&node.nodeType===1;node=node.parentElement){var c=color(getComputedStyle(node).backgroundColor);if(c&&c[3]>.75)return c}return [7,16,24,1]}
  function adapt(){
    Array.prototype.forEach.call(document.body.querySelectorAll('*'),function(el){
      var style=getComputedStyle(el),fg=color(style.color),bg=background(el);
      if(style.display==='none'||style.visibility==='hidden'||!fg||ratio(fg,bg)>=3.5)return;
      el.style.setProperty('color',lum(bg)>.42?'#111827':'#f8fafc','important');
    });
    Array.prototype.forEach.call(document.body.querySelectorAll('*'),function(el){
      var style=getComputedStyle(el);
      if((style.display==='flex'||style.display==='inline-flex')&&style.flexWrap==='nowrap'&&el.scrollWidth>el.clientWidth+6)el.style.setProperty('flex-wrap','wrap');
      if(el.scrollWidth>document.documentElement.clientWidth+6){el.style.setProperty('max-width','100%');el.style.setProperty('min-width','0')}
    });
  }
  function report(){parent.postMessage({type:'folio-artifact-height',height:document.documentElement.scrollHeight},'*')}
  adapt();report();
  new ResizeObserver(report).observe(document.documentElement);
  new MutationObserver(function(){adapt();report()}).observe(document.body,{childList:true,subtree:true});
  addEventListener('resize',function(){adapt();report()});
})();`;

function artifactFrom(source: string): Artifact {
  const css = Array.from(source.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/gi)).map((match) => match[1]).join('\n');
  const js = Array.from(source.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/gi)).map((match) => match[1]).join('\n');
  const body = source.match(/<body[^>]*>([\s\S]*?)<\/body>/i)?.[1] ?? source;
  const html = body.replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '').replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '').trim();
  const rawTitle = source.match(/<h1[^>]*>([\s\S]*?)<\/h1>/i)?.[1] ?? source.match(/<title[^>]*>([\s\S]*?)<\/title>/i)?.[1] ?? 'Interactive response';
  const title = rawTitle.replace(/<[^>]+>/g, '').trim().slice(0, 80) || 'Interactive response';
  const full = `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data: blob:; font-src data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'; connect-src 'none'; media-src data: blob:"><title>${title.replace(/[<>&"]/g, '')}</title><style>${BASE_CSS}\n${css}\n${SAFE_CSS}</style></head><body>${html}<script>${PRELUDE_JS}</script><script>${js}</script><script>${BOOT_JS}</script></body></html>`;
  return { title, html, css, js, full };
}

export function HtmlRenderer({ html }: { html: string }) {
  const artifact = useMemo(() => artifactFrom(html), [html]);
  const [view, setView] = useState<View>('preview');
  const [copied, setCopied] = useState<View | 'full' | null>(null);
  const [height, setHeight] = useState(360);
  const [expanded, setExpanded] = useState(false);
  const [runtimeError, setRuntimeError] = useState(false);
  const frame = useRef<HTMLIFrameElement>(null);
  const shell = useRef<HTMLElement>(null);

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (event.source !== frame.current?.contentWindow) return;
      const data = event.data as { type?: string; height?: number } | null;
      if (data?.type === 'folio-artifact-error') {
        setRuntimeError(true);
        return;
      }
      if (data?.type !== 'folio-artifact-height' || typeof data.height !== 'number') return;
      setHeight(Math.min(760, Math.max(240, data.height + 2)));
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, []);

  useEffect(() => {
    const onFullscreen = () => setExpanded(document.fullscreenElement === shell.current);
    document.addEventListener('fullscreenchange', onFullscreen);
    return () => document.removeEventListener('fullscreenchange', onFullscreen);
  }, []);

  const copy = (kind: View | 'full') => {
    const value = kind === 'preview' || kind === 'full' ? artifact.full : artifact[kind];
    void navigator.clipboard.writeText(value).then(() => {
      setCopied(kind);
      window.setTimeout(() => setCopied((current) => current === kind ? null : current), 1200);
    });
  };

  const source = view === 'html' ? artifact.html : view === 'css' ? artifact.css : artifact.js;
  const toggleExpanded = () => {
    if (document.fullscreenElement === shell.current) {
      void document.exitFullscreen();
      return;
    }
    void shell.current?.requestFullscreen();
  };

  return (
    <section ref={shell} className={`my-3 overflow-hidden border border-accent/35 bg-paper shadow-elevated ${expanded ? 'flex h-screen flex-col rounded-none border-0' : 'rounded-2xl'}`}>
      <header className="flex flex-wrap items-center gap-2 border-b border-rule bg-paper-2/80 px-3 py-2.5">
        <span className="inline-flex items-center gap-1.5 rounded-full border border-accent/35 bg-accent/10 px-2.5 py-1 font-mono text-[10px] font-semibold uppercase tracking-[0.14em] text-accent">
          <Code2 size={12} /> HTML · CSS · JS
        </span>
        <span className="min-w-40 flex-1 truncate text-xs font-medium text-ink">{artifact.title}</span>
        {runtimeError && <span className="rounded-full border border-caution/35 bg-caution/10 px-2 py-1 font-mono text-[9px] uppercase tracking-wide text-caution">saved artifact needs regeneration</span>}
        <div className="flex items-center gap-1 rounded-lg border border-rule bg-paper p-1">
          {(['preview', 'html', 'css', 'js'] as const).map((item) => (
            <button
              key={item}
              type="button"
              onClick={() => setView(item)}
              aria-pressed={view === item}
              className={`rounded-md px-2 py-1 font-mono text-[10px] uppercase transition ${view === item ? 'bg-accent text-paper' : 'text-mute hover:bg-paper-3 hover:text-ink'}`}
            >
              {item === 'preview' ? <span className="inline-flex items-center gap-1"><Eye size={11} />preview</span> : item}
            </button>
          ))}
        </div>
        <button type="button" onClick={() => copy(view)} aria-label={`Copy ${view}`} className="inline-flex h-7 items-center gap-1 rounded-md border border-rule bg-paper px-2 font-mono text-[10px] text-ink-2 hover:bg-paper-3">
          {copied === view ? <Check size={12} /> : <Copy size={12} />}{copied === view ? 'copied' : 'copy'}
        </button>
        <button type="button" onClick={toggleExpanded} title={expanded ? 'Exit full screen' : 'Open full screen'} aria-label={expanded ? 'Exit full screen' : 'Open full screen'} className="inline-flex size-7 items-center justify-center rounded-md border border-rule bg-paper text-mute hover:bg-paper-3 hover:text-ink">
          {expanded ? <Minimize2 size={12} /> : <Maximize2 size={12} />}
        </button>
      </header>
      {view === 'preview' ? (
        <iframe
          ref={frame}
          title={artifact.title}
          srcDoc={artifact.full}
          sandbox="allow-scripts allow-downloads"
          allow="clipboard-write"
          style={{ height: expanded ? '100%' : height }}
          className={`block w-full border-0 bg-[#071018] ${expanded ? 'min-h-0 flex-1' : 'transition-[height] duration-300'}`}
        />
      ) : (
        <pre className={`${expanded ? 'min-h-0 flex-1' : 'max-h-[520px] min-h-60'} overflow-auto bg-[#071018] p-4 font-mono text-xs leading-relaxed text-[#c9dbe6]`}><code>{source || `No ${view.toUpperCase()} was generated.`}</code></pre>
      )}
    </section>
  );
}
