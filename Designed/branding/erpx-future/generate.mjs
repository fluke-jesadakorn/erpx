// Rebuild exports from the hand-drawn, path-only SVG masters.
// bun Designed/branding/erpx-future/generate.mjs
import { readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const root = dirname(fileURLToPath(import.meta.url));
const C = { blue: '#2F69F5', ink: '#111318', muted: '#687181', paper: '#F3F5F8', line: '#DCE1E9', night: '#111722', white: '#FFFFFF', black: '#000000' };
const concepts = [
  { id: 'unified-core', name: 'Unified Core', no: '01', short: 'ONE SYSTEM. SHARED INTELLIGENCE.', thai: 'ทุกส่วนขององค์กร เชื่อมด้วยแกนกลางเดียวกัน', caption: 'Three connected modules. One intelligent core.', note: 'Original direction / แนวทางแรก' },
  { id: 'precision-e', name: 'Precision E', no: '02', selected: true, logoWidth: 586, short: 'PRECISION IN EVERY OPERATION.', thai: 'ERP สีน้ำเงินเดียวกัน · X จางลง', caption: 'One ERP color. One integrated E. A softer X.', note: 'Selected / แบบที่เลือก' },
  { id: 'expansion', name: 'Expansion', no: '03', short: 'CONNECTED WORK. EXPANDING POTENTIAL.', thai: 'รวมพลังจากระบบหลัก สู่การเติบโตที่กว้างขึ้น', caption: 'One foundation. Three paths opening forward.', note: 'Growth-led / เน้นการขยายศักยภาพ' },
];
const esc = (v) => String(v).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;');
const svg = (w, h, title, body, vb = `0 0 ${w} ${h}`) => `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="${vb}" role="img" aria-label="${esc(title)}"><title>${esc(title)}</title>${body}</svg>\n`;
const rect = (x,y,w,h,fill,r=0,stroke='none') => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${r}" fill="${fill}" stroke="${stroke}"/>`;
const line = (x1,y1,x2,y2,color=C.line) => `<path d="M${x1} ${y1}H${x2}" stroke="${color}"/>`;
const txt = (x,y,s,size=18,color=C.ink,weight=500,extra='') => `<text x="${x}" y="${y}" font-family="Avenir Next, Thonburi, Arial, sans-serif" font-size="${size}" font-weight="${weight}" fill="${color}" ${extra}>${esc(s)}</text>`;
const label = (x,y,s,color=C.muted) => txt(x,y,s,11,color,600,'letter-spacing="1.7"');
const group = (x,y,scale,body) => `<g transform="translate(${x} ${y}) scale(${scale})">${body}</g>`;
const mono = (body, color) => body.replace(/fill="#[0-9a-fA-F]{6}"/g, `fill="${color}"`);
const bodyOf = (source) => {
  const found = source.match(/<g id="artwork"[\s\S]*<\/g>/);
  if (!found) throw new Error('Master SVG requires an artwork group');
  return found[0].replace(' id="artwork"', '');
};
const word = bodyOf(await readFile(join(root, 'wordmark.svg'), 'utf8'));
const mark = (c,x,y,size,color=C.blue) => group(x,y,size/128,mono(c.art,color));
const lock = (c,x,y,width,mode='color') => {
  const tint = mode === 'black' ? C.black : mode === 'white' ? C.white : null;
  const lettering = c.word || word;
  if (c.selected) return group(x,y,width/c.logoWidth,group(30,30,1,tint ? mono(lettering,tint) : lettering));
  return group(x,y,width/(c.logoWidth || 656), mark(c,16,16,128,tint || C.blue) + group(168,30,1,tint ? mono(lettering,tint) : lettering));
};
const tileArt = (c) => rect(0,0,128,128,C.blue,28) + mono(c.art,C.white);
const tile = (c,x,y,size) => group(x,y,size/128,tileArt(c));
const nativePixels = (c,size,x,y,scale=1) => {
  const rgba = c.raw.get(size);
  let out = '';
  for (let yy=0; yy<size; yy++) for (let xx=0; xx<size; xx++) {
    const i = (yy*size+xx)*4;
    if (rgba[i+3] === 0) continue;
    out += `<rect x="${x+xx*scale}" y="${y+yy*scale}" width="${scale}" height="${scale}" fill="rgb(${rgba[i]},${rgba[i+1]},${rgba[i+2]})" fill-opacity="${rgba[i+3]/255}"/>`;
  }
  return `<g shape-rendering="crispEdges">${out}</g>`;
};
async function save(name,source,png=true) {
  await writeFile(join(root,`${name}.svg`),source);
  if (png) await sharp(Buffer.from(source)).png().toFile(join(root,`${name}.png`));
}
function ico(frames) {
  const header = Buffer.alloc(6+16*frames.length);
  header.writeUInt16LE(1,2); header.writeUInt16LE(frames.length,4);
  let offset = header.length;
  frames.forEach(({size,png},i) => {
    const at = 6+16*i;
    header[at]=header[at+1]=size;
    header.writeUInt16LE(1,at+4); header.writeUInt16LE(32,at+6);
    header.writeUInt32LE(png.length,at+8); header.writeUInt32LE(offset,at+12);
    offset+=png.length;
  });
  return Buffer.concat([header,...frames.map(f=>f.png)]);
}

for (const c of concepts) {
  const source = await readFile(join(root,c.id,'mark.svg'),'utf8');
  c.art = bodyOf(source); c.raw = new Map(); c.pngs = new Map();
  if (c.selected) c.word = bodyOf(await readFile(join(root,c.id,'wordmark.svg'),'utf8'));
  const logoWidth = c.logoWidth || 656;
  await sharp(Buffer.from(source)).png().toFile(join(root,c.id,'mark.png'));
  for (const mode of ['color','black','white']) {
    const suffix = mode === 'color' ? '' : `-${mode}`;
    if (suffix) await save(`${c.id}/mark${suffix}`,svg(512,512,`ERPX ${c.name} symbol, ${mode}`,mono(c.art,C[mode]),'0 0 128 128'));
    await save(`${c.id}/logo${suffix}`,svg(logoWidth*2,320,`ERPX ${c.name} logo, ${mode}`,lock(c,0,0,logoWidth,mode),`0 0 ${logoWidth} 160`));
  }
  const fav = svg(512,512,`ERPX ${c.name} favicon`,tileArt(c),'0 0 128 128');
  await writeFile(join(root,c.id,'favicon.svg'),fav);
  for (const size of [16,32,48,180,192,512]) {
    const png = await sharp(Buffer.from(fav)).resize(size,size).png().toBuffer();
    await writeFile(join(root,c.id,`icon-${size}.png`),png);
    c.pngs.set(size,png);
    if (size<=48) c.raw.set(size,await sharp(png).ensureAlpha().raw().toBuffer());
  }
  await writeFile(join(root,c.id,'favicon.ico'),ico([16,32,48].map(size=>({size,png:c.pngs.get(size)}))));
}

function comparison() {
  let b = rect(0,0,1800,1230,C.paper);
  b += rect(0,0,1800,270,C.night);
  b += label(62,53,'ERPX  /  IDENTITY COLLECTION  /  2026', '#91A1BA');
  b += txt(60,137,'Enterprise. Multiplied.',66,C.white,600,'letter-spacing="-2.5"');
  b += txt(63,189,'ERP at the core. Intelligence throughout.',23,'#AEBAD0',400);
  b += label(1470,180,'ELECTRIC BLUE', '#9FBAFF');
  [C.blue,C.white,'#000000'].forEach((color,i)=>{b+=rect(1472+i*58,91,44,44,color,22,color==='#000000'?'#43506A':'none');});
  concepts.forEach((c,i) => {
    const x=48+i*572;
    b += rect(x,304,556,850,C.white,18,c.selected?C.blue:C.line);
    b += label(x+28,343,`${c.no}  /  ${c.selected?'SELECTED / REFINED':'ALTERNATIVE'}`,c.selected?C.blue:C.muted);
    b += txt(x+28,389,c.name,34,C.ink,600,'letter-spacing="-0.8"');
    b += txt(x+28,427,c.thai,16,C.muted,400);
    b += mark(c,x+151,461,254);
    b += label(x+28,760,'PRIMARY LOCKUP');
    b += lock(c,x+34,780,488);
    b += rect(x+20,922,516,108,C.night,10);
    b += lock(c,x+91,933,374,'white');
    b += txt(x+28,1080,c.caption,16,C.muted,400);
    b += label(x+28,1119,'FAVICON');
    b += nativePixels(c,16,x+119,1105);
    b += nativePixels(c,32,x+152,1097);
    b += nativePixels(c,48,x+202,1089);
    b += label(x+282,1119,'16 / 32 / 48 PX');
  });
  b += label(50,1195,'CUSTOM VECTOR LETTERING  /  TRANSPARENT SVG  /  ONE-COLOR READY');
  b += txt(1600,1195,'ERP ×',19,C.blue,600);
  return svg(1800,1230,'ERPX — three identity directions',b);
}

function context(c) {
  let b=rect(0,0,1600,1140,C.paper);
  b+=label(48,49,`ERPX / ${c.no} / ${c.name.toUpperCase()}`,C.blue);
  b+=txt(46,111,'Designed for the everyday.',46,C.ink,600,'letter-spacing="-1.5"');
  b+=txt(48,153,c.thai,21,C.muted,400);
  b+=label(48,214,'WEBSITE HEADER + BROWSER TAB');
  // Browser chrome and product header.
  b+=rect(48,236,932,507,C.white,13,C.line);
  b+=rect(49,237,930,42,'#E6EAF0',12);
  b+=rect(164,245,245,35,C.white,8);
  b+=nativePixels(c,16,182,253);
  b+=txt(210,266,'ERPX · Workspace',13,C.ink,500);
  ['#FB827B','#E9C65E','#73C397'].forEach((color,i)=>{b+=rect(68+i*21,252,10,10,color,5);});
  b+=txt(450,264,'erpx.jesadakorn.com',12,C.muted,400);
  b+=lock(c,64,295,173);
  b+=txt(408,321,'Workspace',13,C.ink,600)+txt(528,321,'Operations',13,C.muted)+txt(650,321,'Insights',13,C.muted);
  b+=rect(824,295,128,32,C.blue,6)+txt(843,316,'AI assistant',13,C.white,600);
  b+=line(49,354,979,354);
  b+=label(79,390,'WORKSPACE / OVERVIEW');
  b+=txt(78,431,'Your business, connected.',30,C.ink,600,'letter-spacing="-0.6"');
  b+=txt(80,463,'People, finance and operations. One shared view.',16,C.muted,400);
  [['Revenue','฿2.48M','+18.6% this month'],['Work completed','1,284','Across 12 teams'],['Time saved','248 h','With AI assistance']].forEach((v,i)=>{
    const x=79+288*i;
    b+=rect(x,495,268,129,'#F7F9FC',8,C.line);
    b+=txt(x+19,524,v[0],13,C.muted)+txt(x+18,566,v[1],30,C.ink,600)+txt(x+19,598,v[2],12,i===0?C.blue:C.muted);
  });
  b+=rect(79,650,860,60,'#EDF3FF',8);
  b+=mark(c,94,663,34);
  b+=txt(142,675,'Intelligence across every operation',15,C.ink,600);
  b+=txt(142,697,'Plan, coordinate and move work forward from one system.',13,C.muted,400);
  // Dark website header.
  b+=label(48,789,'DARK HEADER');
  b+=rect(48,813,932,106,C.night,12);
  b+=lock(c,68,839,178,'white');
  b+=txt(410,871,'Workspace',13,C.white,600)+txt(532,871,'Operations',13,'#AEBAD0')+txt(655,871,'Insights',13,'#AEBAD0');
  b+=rect(826,847,128,32,C.blue,6)+txt(845,868,'AI assistant',13,C.white,600);
  // Black-only branded document example; this is not a live ERPX business-document template.
  b+=label(1022,214,'DOCUMENT HEADER / SINGLE INK');
  b+=rect(1022,236,530,746,C.white,0,C.line);
  b+=lock(c,1048,258,233,'black');
  b+=label(1341,295,'ERPX / 2026',C.black);
  b+=line(1055,343,1518,343,'#1A1A1A');
  b+=label(1056,380,'SEPTEMBER 2026',C.black);
  b+=txt(1054,427,'Operations',35,C.black,600)+txt(1054,469,'report',35,C.black,600);
  b+=txt(1056,514,'Connected teams. Measurable progress.',14,'#555555',400);
  b+=label(1056,571,'PERFORMANCE SUMMARY',C.black);
  [['Completed work','1,284'],['Active teams','12'],['Hours saved','248'],['Process coverage','94%']].forEach((v,i)=>{
    const y=611+i*56;
    b+=txt(1056,y,v[0],15,C.black)+txt(1451,y,v[1],17,C.black,600,'text-anchor="end"');
    b+=line(1056,y+19,1516,y+19,'#DDDDDD');
  });
  b+=txt(1056,873,'Prepared for the leadership team',12,'#555555',400);
  b+=line(1056,922,1516,922,'#1A1A1A');
  b+=label(1056,948,'ERPX / BUSINESS OPERATIONS',C.black)+txt(1509,948,'01',12,C.black);
  b+=label(49,986,'FAVICON AT NATIVE SIZE');
  [16,32,48].forEach((size,i)=>{b+=nativePixels(c,size,50+i*100,1004);b+=txt(50+i*100,1077,`${size} px`,12,C.muted);});
  b+=label(410,1009,c.selected?'ONE E. ONE WORDMARK. SOFTER X.':'ERP LEADS. THE MULTIPLIER SUPPORTS.',C.blue);
  b+=txt(410,1041,'Custom paths · Electric blue · Black and white',16,C.muted,400);
  b+=label(48,1110,'BRAND APPLICATION MOCKUPS / ILLUSTRATIVE DATA / NO WEBSITE CHANGES');
  return svg(1600,1140,`ERPX ${c.name} — website, favicon and document mockups`,b);
}

function sizeCheck() {
  let b=rect(0,0,1600,1080,C.paper);
  b+=label(48,48,'ERPX / OPTICAL CHECK',C.blue);
  b+=txt(46,106,'Small size. Clear identity.',43,C.ink,600,'letter-spacing="-1.2"');
  b+=txt(48,145,'Native favicon pixels and nearest-neighbor enlargements. View at 100%.',18,C.muted,400);
  concepts.forEach((c,i)=>{
    const y=185+i*276;
    b+=rect(40,y,1520,254,C.white,12,C.line);
    b+=label(64,y+34,`${c.no} / ${c.name.toUpperCase()}`,C.blue);
    b+=label(64,y+76,'16 / 32 / 48 PX');
    [16,32,48].forEach((size,j)=>{b+=nativePixels(c,size,68+j*74,y+102+(48-size)/2);});
    b+=rect(64,y+175,242,54,C.night,6);
    [16,32,48].forEach((size,j)=>{b+=nativePixels(c,size,74+j*74,y+178+(48-size)/2);});
    b+=nativePixels(c,16,356,y+64,10);
    b+=nativePixels(c,32,552,y+64,5);
    b+=label(356,y+42,'16 PX × 10'); b+=label(552,y+42,'32 PX × 5');
    b+=label(762,y+40,'128 PX LOCKUP');
    b+=lock(c,762,y+57,128);
    b+=label(762,y+128,'256 PX LOCKUP');
    b+=lock(c,758,y+150,256);
    b+=label(1080,y+40,'SINGLE COLOR');
    b+=mark(c,1082,y+79,125,C.black);
    b+=rect(1230,y+68,265,143,C.night,8);
    b+=lock(c,1236,y+110,252,'white');
  });
  b+=label(48,1047,'NO EMBEDDED IMAGES OR FONTS IN THE LOGO MASTERS / ICONS RENDERED FROM VECTOR');
  return svg(1600,1080,'ERPX — favicon pixel inspection and small-size logos',b);
}

await save('comparison',comparison());
await save('size-check',sizeCheck());
for (const c of concepts) await save(`${c.id}/in-context`,context(c));

const chosen = concepts.find(c=>c.selected);
await save('precision-e/preview',svg(1440,820,'ERPX Precision E — integrated E wordmark, standalone E and SVG favicon',
  rect(0,0,1440,820,C.paper)+rect(32,32,1376,344,C.white,14)+
  label(64,73,'PRECISION E / SELECTED IDENTITY',C.blue)+
  txt(985,73,'ERP in one color · X in softer contrast',16,C.muted,400)+
  lock(chosen,160,84,1120)+rect(32,408,980,344,C.night,14)+
  label(64,448,'ON DARK', '#AEBAD0')+lock(chosen,96,484,852,'white')+
  rect(1040,408,368,344,C.white,14)+label(1072,448,'E SYMBOL + FAVICON',C.blue)+
  mark(chosen,1072,494,182)+tile(chosen,1284,540,88)+
  label(64,794,'PATH-ONLY SVG / TRANSPARENT LOGOS / STANDALONE E / X AT 48% OPACITY')));

const cards=concepts.map(c=>`<section id="${c.id}"><header><span class="number">${c.no}</span><div><h2>${c.name}</h2><p>${c.thai}</p></div><span class="badge">${c.selected?'Selected / Refined':'Alternative'}</span></header><div class="stage"><img class="hero-mark" src="${c.id}/mark.svg" alt="${c.name} symbol"><img class="hero-logo" src="${c.id}/logo.svg" alt="ERPX — ${c.name}"></div><div class="variants"><div class="black"><img src="${c.id}/logo-black.svg" alt="Black logo"></div><div class="inverse"><img src="${c.id}/logo-white.svg" alt="White logo"></div></div><div class="downloads"><a download href="${c.id}/logo.svg">Logo SVG ↗</a><a download href="${c.id}/mark.svg">Symbol SVG ↗</a><a download href="${c.id}/favicon.svg">Favicon SVG ↗</a><a href="${c.id}/in-context.png">ดูตัวอย่างการใช้ ↗</a></div><div class="small"><span>Actual pixels</span>${[16,32,48].map(s=>`<img src="${c.id}/icon-${s}.png" width="${s}" height="${s}" alt="${s} pixel favicon">`).join('')}<span>16 · 32 · 48</span></div><details><summary>ตัวอย่างบนเว็บไซต์และหัวเอกสาร</summary><img class="context" src="${c.id}/in-context.png" alt="${c.name} application mockups"></details></section>`).join('');
await writeFile(join(root,'index.html'),`<!doctype html><html lang="th"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>ERPX — Future Identity</title><link rel="icon" href="precision-e/favicon.svg" type="image/svg+xml"><style>*{box-sizing:border-box}body{margin:0;background:#F3F5F8;color:#111318;font:16px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}a{color:inherit;text-decoration:none}a:hover{text-decoration:underline}a:focus-visible,summary:focus-visible{outline:3px solid #2F69F5;outline-offset:5px}.masthead{background:#111722;color:white;padding:64px max(24px,calc((100vw - 1320px)/2)) 68px}.eyebrow{font-size:12px;letter-spacing:2px;text-transform:uppercase;color:#9BB6ED}.masthead h1{font-size:clamp(40px,5vw,76px);line-height:1.05;letter-spacing:-3px;font-weight:600;margin:28px 0 20px}.masthead p{color:#AEBAD0;margin:0}.nav{display:flex;gap:28px;flex-wrap:wrap;margin-top:35px}.nav a{font-size:14px}.nav .zip{color:#A9C3FF}main{max-width:1368px;margin:auto;padding:40px 24px 80px}section{margin-bottom:32px;background:white;border:1px solid #DCE1E9;border-radius:16px;overflow:hidden;scroll-margin-top:24px}section>header{padding:28px 36px;display:flex;gap:20px;align-items:center;border-bottom:1px solid #E7EAF0}.number{color:#2F69F5;font-size:14px}.badge{margin-left:auto;color:#2F69F5;border:1px solid #D9E5FF;border-radius:20px;padding:5px 14px;font-size:12px}h2{font-size:24px;line-height:1.2;margin:0 0 5px;letter-spacing:-.6px}header p{font-size:14px;color:#687181;margin:0}.stage{min-height:330px;display:flex;align-items:center;justify-content:center;gap:100px;padding:48px}.hero-mark{width:200px;max-width:24%}.hero-logo{width:650px;max-width:62%}.variants{display:grid;grid-template-columns:1fr 1fr}.variants>div{height:145px;display:flex;justify-content:center;align-items:center}.variants img{width:340px;max-width:82%}.black{background:#F7F8FA}.inverse{background:#111722}.downloads{padding:24px 36px;display:flex;gap:28px;flex-wrap:wrap;font-size:14px;color:#2457D6}.small{display:flex;align-items:center;gap:22px;padding:0 36px 24px;font-size:12px;color:#687181}.small img{flex:none}details{border-top:1px solid #E7EAF0;padding:20px 36px}summary{cursor:pointer;font-size:14px;color:#687181}.context{display:block;width:100%;height:auto;margin-top:20px}.footer{display:flex;gap:25px;font-size:14px;color:#687181;flex-wrap:wrap}@media(max-width:700px){.masthead h1{letter-spacing:-1.5px}.stage{gap:30px;min-height:230px;padding:25px}.hero-mark{width:120px}.variants>div{height:100px}.badge{display:none}section>header{padding:22px}.downloads{padding:22px;gap:18px}.small{padding:0 22px 22px;gap:14px}details{padding:20px}.variants img{max-width:90%}}</style></head><body><div class="masthead"><div class="eyebrow">ERPX / Identity collection / 2026</div><h1>Enterprise.<br>Multiplied.</h1><p>ERP เป็นแกนหลัก · AI เชื่อมทุกส่วน · X คือพลังทวีคูณ</p><nav class="nav" aria-label="แนวทางโลโก้">${concepts.map(c=>`<a href="#${c.id}">${c.no} ${c.name}</a>`).join('')}<a class="zip" download href="precision-e-svg-kit.zip">Precision E · SVG ทั้งชุด ↗</a><a class="zip" download href="erpx-future-logo-kit.zip">รวมทุกแนวทาง ↗</a></nav></div><main>${cards}<div class="footer"><a href="comparison.png">ภาพเปรียบเทียบ</a><a href="size-check.png">ตรวจขนาดเล็ก</a><a href="README.md">คู่มือใช้งาน</a><span>Original path-only SVG · Website unchanged</span></div></main></body></html>\n`);
console.log('Generated three logo kits, comparison, size check, application mockups and index.html.');
