import type { WindowsPolicyDefinition } from "./directory-operations";
const xml = (v: string) =>
  v.replace(
    /[&<>"']/g,
    (c) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&apos;",
      })[c]!,
  );
export type PreferenceFile = {
  path:
    | "User/Preferences/Drives/Drives.xml"
    | "User/Preferences/Printers/Printers.xml";
  xml: string;
  extension: string;
  toolExtension: string;
};
/** Group Policy Preferences are compiled separately from registry-based policies. No credentials. */
export function compilePreferences(
  p: WindowsPolicyDefinition,
  changed: string,
): PreferenceFile[] {
  const result: PreferenceFile[] = [];
  result.push({
    path: "User/Preferences/Drives/Drives.xml",
    extension: "{5794DAFD-BE60-433f-88A2-1A31939AC01F}",
    toolExtension: "{2EA1A81B-48E5-45e9-8BB7-A6E3AC170006}",
    xml:
      '<?xml version="1.0" encoding="utf-8"?><Drives clsid="{8FDDCC1A-0C3C-43cd-A6B4-71A6DF20DA8C}">' +
      p.drives
        .map(
          (d) =>
            '<Drive clsid="{935D1B74-9CB8-4e3c-9914-7DD559B7A417}" name="' +
            xml(d.letter) +
            ':" status="' +
            xml(d.letter) +
            ':" image="2" changed="' +
            xml(changed) +
            '" uid="{' +
            d.id.toUpperCase() +
            '}" userContext="1"><Properties action="' +
            (d.remove ? "D" : "U") +
            '" thisDrive="NOCHANGE" allDrives="NOCHANGE" path="' +
            xml(d.path) +
            '" label="' +
            xml(d.label) +
            '" persistent="1" useLetter="1" letter="' +
            xml(d.letter) +
            '"/></Drive>',
        )
        .join("") +
      "</Drives>",
  });
  result.push({
    path: "User/Preferences/Printers/Printers.xml",
    extension: "{BC75B1ED-5833-4858-9BB8-CBF0B166DF9D}",
    toolExtension: "{A8C42CEA-CDB8-4388-97F4-5831F933DA84}",
    xml:
      '<?xml version="1.0" encoding="utf-8"?><Printers clsid="{1F577D12-3D1B-471e-A1B7-060317597B9C}">' +
      p.printers
        .map(
          (d) =>
            '<SharedPrinter clsid="{9A5E9697-9095-436d-A0EE-4D128FDFBCE5}" name="' +
            xml(d.path) +
            '" status="' +
            xml(d.path) +
            '" image="2" changed="' +
            xml(changed) +
            '" uid="{' +
            d.id.toUpperCase() +
            '}" userContext="1"><Properties action="' +
            (d.remove ? "D" : "U") +
            '" path="' +
            xml(d.path) +
            '" default="' +
            (d.isDefault ? "1" : "0") +
            '" persistent="1" port=""/></SharedPrinter>',
        )
        .join("") +
      "</Printers>",
  });
  return result;
}
