// Development-only factual-data import. Preserve the upstream BSD notice.
import fs from 'node:fs';
import crypto from 'node:crypto';
import dingbatModule from 'dingbat-to-unicode/dist/dingbats.js';
const expected='5d99a6ab462352f6a6b2c2ce1dd9062821a0c211b41bbe7249bef631df1e05d6';
const source=new URL(import.meta.resolve('dingbat-to-unicode/dist/dingbats.js'));
const actual=crypto.createHash('sha256').update(fs.readFileSync(source)).digest('hex');
const version=JSON.parse(fs.readFileSync(new URL(import.meta.resolve('dingbat-to-unicode/package.json')),'utf8')).version;
if(actual!==expected||version!=='1.0.2')throw new Error('Preserve the captured import; review an upstream update separately');
const map=dingbatModule.default;
const rows=Object.entries(map).sort(([a],[b])=>a.localeCompare(b)).flatMap(([font,values])=>Object.entries(values).sort(([a],[b])=>Number(a)-Number(b)).map(([code,unicode])=>`    (${JSON.stringify(font)},${code},${unicode}),`));
const text=`// Factual mapping data from dingbat-to-unicode 1.0.2, BSD-2-Clause.
// Copyright (c) 2021 Michael Williamson; see ../../../licenses/dingbat-to-unicode-BSD-2-Clause.txt.
// Upstream: https://github.com/mwilliamson/dingbat-to-unicode
// Source dist/dingbats.js SHA256 ${expected}
#[rustfmt::skip]
const MAP:&[(&str,u32,u32)]=&[
${rows.join('\n')}
];
pub(super) fn lookup(font: &str, code: u32) -> Option<char> {
    let font = font.to_uppercase();
    MAP.binary_search_by(|(name, value, _)| name.cmp(&font.as_str()).then(value.cmp(&code)))
        .ok()
        .and_then(|index| char::from_u32(MAP[index].2))
}
`;
fs.writeFileSync('packages/server-legal/src/adapters/document_docx/dingbats.rs',text);
const license=new URL(import.meta.resolve('dingbat-to-unicode/LICENSE'));
fs.copyFileSync(license,'packages/server-legal/licenses/dingbat-to-unicode-BSD-2-Clause.txt');
console.log(`Imported ${rows.length} attributed mapping entries`);
