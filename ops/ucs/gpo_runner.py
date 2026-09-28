#!/usr/bin/env python3
"""Finite UCS/Samba GPO API. No shell execution, arbitrary registry paths or XML input.

Samba Python bindings are supplied by UCS. See README.md for the trust boundary.
The HTTPS service is intentionally single-threaded; flock also serializes processes.
"""
import argparse
import base64
import configparser
import copy
import fcntl
import hashlib
import hmac
import http.server
import io
import json
import os
from pathlib import Path
import re
import secrets
import ssl
import time
import urllib.parse
import urllib.request
import uuid
import xml.etree.ElementTree as ET

APPLY_GUID = "edacfd8f-ffb3-11d1-b41d-00a0c968f939"
REG_CSE = "[{35378EAC-683F-11D2-A89A-00C04FBBCFA2}{D02B1F72-3407-48AE-BA88-E8213C6761F1}]"
GPP = {
    "drives": ("Drives", "Drive", "{8FDDCC1A-0C3C-43cd-A6B4-71A6DF20DA8C}", "{935D1B74-9CB8-4e3c-9914-7DD559B7A417}", "[{5794DAFD-BE60-433F-88A2-1A31939AC01F}{2EA1A81B-48E5-45E9-8BB7-A6E3AC170006}]"),
    "printers": ("Printers", "SharedPrinter", "{1F577D12-3D1B-471E-A1B7-060317597B9C}", "{9A5E9697-9095-436D-A0EE-4D128FDFBCE5}", "[{BC75B1ED-5833-4858-9BB8-CBF0B166DF9D}{A8C42CEA-CDB8-4388-97F4-5831F933DA84}]"),
}
GPO_KINDS = {"gpo.publish", "gpo.link", "gpo.order", "gpo.inheritance", "gpo.filter"}
DIRECTORY_KINDS = {"account.create", "account.update", "account.disable", "account.enable", "group.create", "group.members", "ou.create", "ou.update", "object.move"}
KINDS = GPO_KINDS | DIRECTORY_KINDS

class Conflict(Exception):
    def __init__(self, code):
        self.code = code
        super().__init__(code)

def require(ok, code="DIRECTORY_ACTION_INVALID"):
    if not ok:
        raise Conflict(code)

def remove_represented_apply_aces(sddl, represented_sids=None):
    """Remove only Apply ACEs for groups visible in the current inventory.

    A newly created GPO passes None and replaces its default filter. An update
    passes the represented group SIDs, preserving unknown/out-of-scope ACEs.
    """
    for ace in re.findall(r"\([^()]*\)", sddl):
        fields = ace[1:-1].split(";")
        if (len(fields) >= 6 and fields[0] == "OA" and fields[3].lower() == APPLY_GUID
                and (represented_sids is None or fields[5] in represented_sids)):
            sddl = sddl.replace(ace, "")
    return sddl

def fingerprint(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()

def atomic_json(path, value):
    tmp = path.with_suffix(".tmp")
    with os.fdopen(os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "w") as f:
        json.dump(value, f, sort_keys=True)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)
    fd = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)

def validate_policy(p, settings_only=False):
    require(isinstance(p, dict) and p.get("scope") in ("user", "computer"))
    require(isinstance(p.get("name"), str) and 2 <= len(p["name"]) <= 120)
    require(p.get("usb", "unchanged") in ("unchanged", "allow", "deny"))
    require(p.get("lockSeconds") is None or type(p["lockSeconds"]) is int and 60 <= p["lockSeconds"] <= 86400)
    require(p.get("wallpaperStyle", "10") in ("0", "2", "6", "10"))
    if p.get("wallpaper"):
        require(bool(re.fullmatch(r"(?:[A-Za-z]:\\|\\\\)[^\r\n]+\.(?:bmp|jpe?g)", p["wallpaper"], re.I)))
    if not settings_only:
        require(p.get("targetOuId") and p.get("filterGroupId"))
    if p["scope"] == "computer":
        require(p.get("computerTargetId"))
        if not settings_only:
            require(p["computerTargetId"] in (p["targetOuId"], p["filterGroupId"]))
        require(not p.get("wallpaper") and not p.get("drives") and not p.get("printers"))
    for kind in GPP:
        items = p.get(kind, [])
        require(isinstance(items, list) and len(items) <= 20)
        seen = set()
        for item in items:
            require(isinstance(item, dict))
            ident = str(uuid.UUID(item["id"]))
            require(ident not in seen)
            seen.add(ident)
            require(isinstance(item.get("path"), str) and len(item["path"]) <= 500 and bool(re.fullmatch(r"\\\\[^\\\s]+\\[^\r\n]+", item["path"])))
            require(type(item.get("remove", False)) is bool)
            if kind == "drives":
                require(bool(re.fullmatch("[D-Z]", item.get("letter", ""))))
                require(isinstance(item.get("label"), str) and len(item["label"]) <= 500)
            else:
                require(type(item.get("isDefault", False)) is bool)
    require(len({d["letter"] for d in p.get("drives", [])}) == len(p.get("drives", [])))
    require(sum(bool(d.get("isDefault") and not d.get("remove")) for d in p.get("printers", [])) <= 1)
    return p

def registry_values(p):
    """Finite supported keys; value type numbers are Windows REG_SZ/REG_DWORD."""
    out = []
    if p.get("usb", "unchanged") != "unchanged":
        out.append((r"Software\Policies\Microsoft\Windows\RemovableStorageDevices", "Deny_All", 4, int(p["usb"] == "deny")))
    if p.get("lockSeconds") is not None:
        if p["scope"] == "computer":
            out.append((r"Software\Microsoft\Windows\CurrentVersion\Policies\System", "InactivityTimeoutSecs", 4, p["lockSeconds"]))
        else:
            for key, value in {"ScreenSaveActive": "1", "ScreenSaverIsSecure": "1", "ScreenSaveTimeOut": str(p["lockSeconds"]), "SCRNSAVE.EXE": "scrnsave.scr"}.items():
                out.append((r"Software\Policies\Microsoft\Windows\Control Panel\Desktop", key, 1, value))
    if p.get("wallpaper"):
        for key, value in {"Wallpaper": p["wallpaper"], "WallpaperStyle": p.get("wallpaperStyle", "10")}.items():
            out.append((r"Software\Microsoft\Windows\CurrentVersion\Policies\System", key, 1, value))
    return out

def merge_preferences(original, kind, items, owned_ids):
    """Patch only selected UIDs and previously managed UIDs. Retain every other node."""
    root_name, item_name, root_clsid, item_clsid, _ = GPP[kind]
    require(not original or b"<!DOCTYPE" not in original.upper(), "DIRECTORY_XML_UNSUPPORTED")
    root = ET.fromstring(original) if original else ET.Element(root_name, {"clsid": root_clsid})
    require(root.tag == root_name, "DIRECTORY_XML_UNSUPPORTED")
    replaced = {str(uuid.UUID(x)).lower() for x in owned_ids} | {str(uuid.UUID(x["id"])).lower() for x in items}
    originals = {child.get("uid", "").strip("{}").lower(): copy.deepcopy(child) for child in root if child.tag == item_name}
    for child in list(root):
        uid = child.get("uid", "").strip("{}").lower()
        if child.tag == item_name and uid in replaced:
            root.remove(child)
    for item in items:
        node = originals.get(str(uuid.UUID(item["id"])).lower(), ET.Element(item_name))
        node.attrib.update({"clsid": item_clsid, "uid": "{" + str(uuid.UUID(item["id"])).upper() + "}", "name": item["letter"]+":" if kind == "drives" else item["path"], "userContext": "1", "changed": time.strftime("%Y-%m-%d %H:%M:%S", time.gmtime())})
        root.append(node)
        attrs = {"action": "D" if item.get("remove") else "U", "path": item["path"], "persistent": "1"}
        if kind == "drives":
            attrs.update({"letter": item["letter"], "label": item["label"], "useLetter": "1", "thisDrive": "NOCHANGE", "allDrives": "NOCHANGE"})
        else:
            attrs.update({"default": "1" if item.get("isDefault") else "0", "port": ""})
        properties = node.find("Properties")
        if properties is None:
            properties = ET.SubElement(node, "Properties")
        properties.attrib.update(attrs)
    return ET.tostring(root, encoding="utf-8", xml_declaration=True)

def parse_links(value):
    if not value:
        return []
    matches = list(re.finditer(r"\[LDAP://([^;\]]+);([0-3])\]", value, re.I))
    require("".join(m.group(0) for m in matches) == value, "DIRECTORY_LINKS_UNSUPPORTED")
    return [{"dn": m.group(1), "options": int(m.group(2))} for m in reversed(matches)]

def encode_links(links):
    return "".join("[LDAP://%s;%d]" % (l["dn"], l["options"]) for l in reversed(links))

class SambaAdapter:
    def __init__(self, config):
        import ldb
        from samba.param import LoadParm
        from samba.credentials import Credentials
        from samba.samdb import SamDB
        from samba.auth import system_session
        from samba.netcmd.gpcommon import smb_connection
        self.ldb = ldb
        self.config = config
        self.lp = LoadParm()
        if config.get("smb_conf"):
            self.lp.load(config["smb_conf"])
        else:
            self.lp.load_default()
        self.lp.set("tls verify peer", "ca_and_name")
        self.lp.set("tls cafile", config["ca_file"])
        require(config["ldap_url"].startswith("ldaps://"), "DIRECTORY_TLS_REQUIRED")
        secret = json.loads(Path(config["credentials_file"]).read_text())
        creds = Credentials()
        creds.guess(self.lp)
        creds.set_domain(str(self.lp.get("workgroup")))
        creds.set_username(secret["username"])
        creds.set_password(secret["password"])
        self.db = SamDB(url=config["ldap_url"], lp=self.lp, credentials=creds, session_info=system_session())
        self.smb = smb_connection(config["samba_host"], "sysvol", lp=self.lp, creds=creds)
        self.base = str(self.db.get_default_basedn())
        self.policy_base = "CN=Policies,CN=System," + self.base
        self.realm = str(self.lp.get("realm")).lower()
        self.state = Path(config["state_directory"])
        mapping_file = config.get("identity_mapping_file")
        self.mappings = json.loads(Path(mapping_file).read_text()) if mapping_file else None
        self.secret = secret
        self.expected_attributes = {}
        self.expected_files = {}

    def search(self, base=None, expression="(objectClass=*)", attrs=None, scope=None):
        return self.db.search(base=base or self.base, expression=expression, scope=self.ldb.SCOPE_SUBTREE if scope is None else scope, attrs=attrs or ["*", "nTSecurityDescriptor"], controls=["sd_flags:1:7"])

    def attr(self, row, key, default=""):
        return str(row[key][0]) if key in row else default

    def attrs(self, row, key):
        return [bytes(v) if isinstance(v, bytes) else str(v) for v in row[key]] if key in row else []

    def guid(self, row):
        from samba.ndr import ndr_unpack
        from samba.dcerpc.misc import GUID
        return str(ndr_unpack(GUID, bytes(row["objectGUID"][0])))

    def allowed_dn(self, dn):
        candidate = self.ldb.Dn(self.db, dn)
        return any(candidate == self.ldb.Dn(self.db, base) or candidate.is_child_of(self.ldb.Dn(self.db, base)) for base in self.config["write_base_dns"])

    def files(self, guid):
        """Bounded full GPT backup; no silent omission of unknown policy files."""
        root = self.realm + "\\Policies\\" + guid
        result = {}
        def walk(directory, relative=""):
            for entry in self.smb.list(directory, "*"):
                name = entry["name"]
                if name in (".", ".."):
                    continue
                require("\\" not in name and "/" not in name and name not in ("", ".", ".."))
                rel = relative + name
                if entry["attrib"] & 0x10:
                    walk(directory + "\\" + name, rel + "\\")
                else:
                    data = self.smb.loadfile(directory + "\\" + name)
                    require(len(data) <= 16_777_216 and len(result) < 5000, "DIRECTORY_GPO_TOO_LARGE")
                    result[rel] = base64.b64encode(data).decode()
        walk(root)
        return result

    def snapshot(self, row, gpo=False):
        attributes = {k: [base64.b64encode(bytes(x)).decode() for x in row[k]] for k in row.keys() if k != "dn"}
        value = {"dn": str(row.dn), "attributes": attributes}
        if gpo:
            value["files"] = self.files(self.attr(row, "name"))
        return value

    def observed_definitions(self, obj, files):
        from samba.ndr import ndr_unpack
        from samba.dcerpc import preg
        definitions = copy.deepcopy(obj["properties"].get("definitions", {}))
        for scope, section in (("user", "User"), ("computer", "Machine")):
            path = next((f for f in files if f.lower() == (section+"\\Registry.pol").lower()), None)
            if not path and scope not in definitions:
                continue
            p = definitions.get(scope, {"id": obj["id"], "name": obj["name"], "scope": scope, "usb": "unchanged", "lockSeconds": None, "wallpaper": None, "wallpaperStyle": "10", "drives": [], "printers": []})
            if path:
                pol = ndr_unpack(preg.file, base64.b64decode(files[path]))
                values = {(e.keyname.lower(), e.valuename.lower()): e.data for e in pol.entries}
                def value(key, name):
                    return values.get((key.lower(), name.lower()))
                usb = value(r"Software\Policies\Microsoft\Windows\RemovableStorageDevices", "Deny_All")
                if usb in (0, 1): p["usb"] = "deny" if usb else "allow"
                lock = value(r"Software\Microsoft\Windows\CurrentVersion\Policies\System", "InactivityTimeoutSecs") if scope == "computer" else value(r"Software\Policies\Microsoft\Windows\Control Panel\Desktop", "ScreenSaveTimeOut")
                if lock is not None and str(lock).isdigit() and 60 <= int(lock) <= 86400: p["lockSeconds"] = int(lock)
                if scope == "user":
                    wallpaper = value(r"Software\Microsoft\Windows\CurrentVersion\Policies\System", "Wallpaper")
                    style = value(r"Software\Microsoft\Windows\CurrentVersion\Policies\System", "WallpaperStyle")
                    if isinstance(wallpaper, str): p["wallpaper"] = wallpaper
                    if style in ("0", "2", "6", "10"): p["wallpaperStyle"] = style
            if scope == "user":
                for kind, (folder, tag, _, _, _) in GPP.items():
                    path = next((f for f in files if f.lower() == ("User\\Preferences\\%s\\%s.xml" % (folder, folder)).lower()), None)
                    if not path: continue
                    xml = base64.b64decode(files[path]); require(b"<!DOCTYPE" not in xml.upper(), "DIRECTORY_XML_UNSUPPORTED")
                    items = []
                    for node in ET.fromstring(xml).findall(tag):
                        props = node.find("Properties")
                        try:
                            uid = str(uuid.UUID(node.get("uid", "").strip("{}")))
                            if props is None or props.get("action") not in ("U", "D"): continue
                            item = {"id": uid, "path": props.get("path", ""), "remove": props.get("action") == "D"}
                            if kind == "drives": item.update(letter=props.get("letter", ""), label=props.get("label", ""))
                            else: item["isDefault"] = props.get("default") == "1"
                            validate_policy({**p, "targetOuId": "observed", "filterGroupId": "observed", kind: [item]})
                            items.append(item)
                        except (ValueError, Conflict): continue
                    if len(items) <= 20: p[kind] = items
            definitions[scope] = p
        return definitions

    def inventory(self):
        rows = self.search(expression="(|(objectClass=organizationalUnit)(objectClass=group)(objectClass=user)(objectClass=groupPolicyContainer))")
        objects = []
        for row in rows:
            classes = [str(v).lower() for v in row["objectClass"]]
            kind = "gpo" if "grouppolicycontainer" in classes else "ou" if "organizationalunit" in classes else "group" if "group" in classes else "computer" if "computer" in classes else "account"
            guid = self.guid(row)
            properties = {}
            if kind != "gpo":
                if not self.allowed_dn(str(row.dn)):
                    continue
                if self.mappings is not None:
                    mapping = next((m for m in self.mappings if m["sambaGuid"].lower() == guid.lower()), None)
                    require(mapping, "DIRECTORY_IDENTITY_MAPPING_UNVERIFIED")
                    # Validate the trusted pair against UDM on every read, never infer it by a display name.
                    path = self.config["udm_url"].rstrip("/") + "/" + mapping["udmModule"] + "/" + urllib.parse.quote(mapping["udmDn"], safe="")
                    request = urllib.request.Request(path, headers={"Accept": "application/json", "Authorization": "Basic " + base64.b64encode((self.secret["udmUsername"]+":"+self.secret["udmPassword"]).encode()).decode()})
                    with urllib.request.urlopen(request, context=ssl.create_default_context(cafile=self.config["ca_file"]), timeout=15) as response:
                        udm = json.load(response)
                    require(udm.get("uuid") == mapping["udmUuid"], "DIRECTORY_IDENTITY_MAPPING_UNVERIFIED")
                    properties["udmUuid"] = mapping["udmUuid"]
                properties.update({
                    "firstname": self.attr(row, "givenName"),
                    "lastname": self.attr(row, "sn"),
                    "displayName": self.attr(row, "displayName"),
                    "description": self.attr(row, "description"),
                    "mailPrimaryAddress": self.attr(row, "mail"),
                })
                if kind == "account":
                    properties["disabled"] = bool(int(self.attr(row, "userAccountControl", "0")) & 2)
            else:
                if not self.gpo_allowed(self.attr(row, "name")):
                    continue
                managed = self.state / ("policy-" + guid + ".json")
                if managed.exists():
                    saved = json.loads(managed.read_text())
                    definitions = {saved["scope"]: saved} if "scope" in saved else saved
                    properties["definitions"] = definitions
                    properties["definition"] = definitions.get("user") or definitions.get("computer")
                from samba.ndr import ndr_unpack
                from samba.dcerpc import security
                sd = ndr_unpack(security.descriptor, bytes(row["nTSecurityDescriptor"][0]))
                properties["applyGroupSids"] = []
                for ace in re.findall(r"\([^()]*\)", sd.as_sddl()):
                    fields = ace[1:-1].split(";")
                    if len(fields) >= 6 and fields[0] == "OA" and fields[3].lower() == APPLY_GUID:
                        properties["applyGroupSids"].append(fields[5])
            snapshot = self.snapshot(row, kind == "gpo")
            if kind == "gpo":
                properties["definitions"] = self.observed_definitions({"id":guid,"name":self.attr(row,"displayName") or self.attr(row,"name"),"properties":properties}, snapshot["files"])
            properties["blockInheritance"] = bool(int(self.attr(row, "gPOptions", "0")) & 1)
            properties["rawLinks"] = parse_links(self.attr(row, "gPLink"))
            properties["sid"] = self.sid(row) if "objectSid" in row else None
            objects.append({"id": guid, "dn": str(row.dn), "name": self.attr(row, "sAMAccountName") or self.attr(row, "displayName") or self.attr(row, "name"), "kind": kind, "parentId": None, "version": fingerprint(snapshot), "module": "ad" if self.mappings is None else "samba", "properties": properties, "memberIds": [], "_members": [str(v) for v in row["member"]] if "member" in row else []})
        by_dn = {o["dn"].lower(): o for o in objects}
        for o in objects:
            parent = str(self.ldb.Dn(self.db, o["dn"]).parent()).lower()
            o["parentId"] = by_dn.get(parent, {}).get("id")
            o["memberIds"] = [by_dn[d.lower()]["id"] for d in o.pop("_members") if d.lower() in by_dn]
            links = o["properties"].pop("rawLinks")
            o["properties"]["gpoLinks"] = [{"gpoId": by_dn[l["dn"].lower()]["id"], "enabled": not bool(l["options"] & 1), "enforced": bool(l["options"] & 2)} for l in links if l["dn"].lower() in by_dn]
            o["properties"]["unmanagedLinkCount"] = sum(l["dn"].lower() not in by_dn for l in links)
            if o["kind"] == "gpo":
                linked = self.search(expression="(gPLink=*%s*)" % self.ldb.binary_encode(o["dn"]), attrs=["objectGUID", "gPLink"])
                require(all(str(r.dn).lower() in by_dn for r in linked), "DIRECTORY_GPO_SCOPE_INCOMPLETE")
                o["memberIds"] = [by_dn[str(r.dn).lower()]["id"] for r in linked]
                o["properties"]["securityFilterGroupIds"] = [g["id"] for g in objects if g["kind"] == "group" and g["properties"].get("sid") in o["properties"]["applyGroupSids"]]
                for scope, p in o["properties"]["definitions"].items():
                    p["name"] = o["name"]
                    p["targetOuId"] = p.get("targetOuId") or next(iter(o["memberIds"]), "unselected")
                    p["filterGroupId"] = p.get("filterGroupId") or next(iter(o["properties"]["securityFilterGroupIds"]), "unselected")
                    p["computerTargetId"] = p.get("computerTargetId") or (p["targetOuId"] if scope == "computer" else None)
                o["properties"]["definition"] = o["properties"]["definitions"].get("user") or o["properties"]["definitions"].get("computer")
        # Shared targets are part of the preview version: changing an OU link or
        # group membership must invalidate a previously previewed settings patch.
        for gpo in (o for o in objects if o["kind"] == "gpo"):
            impact = set(gpo["memberIds"] + gpo["properties"]["securityFilterGroupIds"])
            changed = True
            while changed:
                previous = set(impact)
                for obj in objects:
                    if obj["id"] in impact:
                        impact.update(obj["memberIds"])
                    elif obj["parentId"] in impact or (obj["kind"] == "group" and impact.intersection(obj["memberIds"])):
                        impact.add(obj["id"])
                changed = impact != previous
            gpo["version"] = fingerprint({"policy": gpo["version"], "targets": sorted((o["id"], o["version"]) for o in objects if o["id"] in impact and o["kind"] != "gpo")})
        fields = [] if self.mappings is not None else [{"name": name, "type": "text", "read": True, "write": True} for name in ("firstname", "lastname", "displayName", "description", "mailPrimaryAddress")]
        return {"objects": objects, "fields": fields, "providerVersion": "UCS Samba GPO runner v1" if self.mappings is not None else "Samba AD runner v1", "observedAt": int(time.time()*1000), "complete": True}

    def sid(self, row):
        from samba.ndr import ndr_unpack
        from samba.dcerpc import security
        return str(ndr_unpack(security.dom_sid, bytes(row["objectSid"][0])))

    def gpo_allowed(self, name):
        return name.upper() in [v.upper() for v in self.config.get("allowed_gpo_ids", [])] or (self.state / ("created-" + name.strip("{}") + ".json")).exists()

    def row(self, guid):
        rows = self.search(base="<GUID=%s>" % str(uuid.UUID(guid)), scope=self.ldb.SCOPE_BASE)
        require(len(rows) == 1, "DIRECTORY_IDENTITY_CHANGED")
        return rows[0]

    def modify(self, row, values):
        m = self.ldb.Message()
        m.dn = row.dn
        for key, value in values.items():
            m[key] = self.ldb.MessageElement(value, self.ldb.FLAG_MOD_REPLACE, key)
        self.db.modify(m, controls=["sd_flags:1:4"] if "nTSecurityDescriptor" in values else [])
        self.expected_attributes.setdefault(self.guid(row), {}).update(values)

    def create_directory_object(self, action, operation, parent):
        marker = "ERPX operation " + operation
        if action["kind"] == "account.create":
            username = action.get("username", "")
            require(bool(re.fullmatch(r"[A-Za-z][A-Za-z0-9._-]{1,63}", username)), "DIRECTORY_NAME_INVALID")
            self.db.newuser(username, secrets.token_urlsafe(32), userou=str(parent.dn), surname=action.get("lastName"), givenname=action.get("firstName"), description=marker)
            self.db.disable_account("sAMAccountName=%s" % self.ldb.binary_encode(username))
            dn = "CN=" + username + "," + str(parent.dn)
        elif action["kind"] == "group.create":
            name = action.get("name", "")
            require(bool(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9 ._-]{1,99}", name)), "DIRECTORY_NAME_INVALID")
            self.db.newgroup(name, groupou=str(parent.dn), description=marker)
            dn = "CN=" + name + "," + str(parent.dn)
        else:
            name = action.get("name", "")
            require(bool(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9 ._-]{1,99}", name)), "DIRECTORY_NAME_INVALID")
            dn = "OU=" + name + "," + str(parent.dn)
            self.db.add({"dn": dn, "objectClass": "organizationalUnit", "description": marker})
        rows = self.search(base=dn, scope=self.ldb.SCOPE_BASE)
        require(len(rows) == 1 and self.attr(rows[0], "description") == marker, "DIRECTORY_OBJECT_EXISTS")
        return rows[0]

    def apply_directory(self, action, operation, resolved):
        kind = action["kind"]
        target = resolved.get(action.get("objectId"))
        if kind in ("account.create", "group.create", "ou.create"):
            return self.create_directory_object(action, operation, resolved[action["parentId"]])
        require(target is not None, "DIRECTORY_TARGET_INVALID")
        if kind == "account.update":
            mapping = {"firstname": "givenName", "lastname": "sn", "displayName": "displayName", "description": "description", "mailPrimaryAddress": "mail"}
            require(action.get("fields") and all(k in mapping for k in action["fields"]), "DIRECTORY_FIELD_UNSUPPORTED")
            self.modify(target, {mapping[k]: v for k, v in action["fields"].items()})
        elif kind == "account.disable":
            self.db.disable_account("objectGUID=%s" % self.guid(target))
        elif kind == "account.enable":
            self.db.enable_account("objectGUID=%s" % self.guid(target))
        elif kind == "group.members":
            members = [str(v) for v in target["member"]] if "member" in target else []
            for member_id in action.get("memberIds", []):
                dn = str(resolved[member_id].dn)
                if action.get("action") == "add" and dn.lower() not in {v.lower() for v in members}:
                    members.append(dn)
                elif action.get("action") == "remove":
                    members = [v for v in members if v.lower() != dn.lower()]
            self.modify(target, {"member": members})
        elif kind == "ou.update":
            require(isinstance(action.get("description"), str) and len(action["description"]) <= 500, "DIRECTORY_ACTION_INVALID")
            self.modify(target, {"description": action["description"]})
        elif kind == "object.move":
            parent = resolved[action["parentId"]]
            rdn = str(target.dn).split(",", 1)[0]
            self.db.rename(target.dn, self.ldb.Dn(self.db, rdn + "," + str(parent.dn)))
        else:
            require(False, "DIRECTORY_ACTION_UNSUPPORTED")
        return self.row(self.guid(target))

    def write_file(self, guid, relative, value):
        from samba.netcmd.gpcommon import create_directory_hier
        path = self.realm + "\\Policies\\" + guid + "\\" + relative
        create_directory_hier(self.smb, path.rsplit("\\", 1)[0])
        self.smb.savefile(path, value)
        require(self.smb.loadfile(path) == value, "DIRECTORY_READBACK_FAILED")
        self.expected_files[path] = value

    def filter(self, row, groups, represented_sids=None):
        from samba.dcerpc import security
        from samba.ndr import ndr_unpack, ndr_pack
        from samba.ntacls import dsacl2fsacl
        sd = ndr_unpack(security.descriptor, bytes(row["nTSecurityDescriptor"][0]))
        # The inventory intentionally excludes objects outside the connection's
        # allowed DNs. Their Apply ACEs are unmanaged and must survive a filter
        # update for a Position mapping.
        sddl = remove_represented_apply_aces(sd.as_sddl(), represented_sids)
        suffix = ""
        if "S:" in sddl:
            sddl, tail = sddl.split("S:", 1)
            suffix = "S:" + tail
        for group in groups:
            sid = self.sid(group)
            sddl += "(OA;;CR;%s;;%s)(A;;RPLCRC;;;%s)" % (APPLY_GUID, sid, sid)
        patched = security.descriptor.from_sddl(sddl + suffix, security.dom_sid(self.db.get_domain_sid()))
        self.modify(row, {"nTSecurityDescriptor": ndr_pack(patched)})
        fs = dsacl2fsacl(patched.as_sddl(), security.dom_sid(self.db.get_domain_sid()), as_sddl=False)
        self.smb.set_acl(self.realm+"\\Policies\\"+self.attr(row, "name"), fs, security.SECINFO_DACL | security.SECINFO_PROTECTED_DACL)

    def create(self, operation, name):
        from samba.netcmd.gpcommon import create_directory_hier
        from samba.dcerpc import security
        from samba.ndr import ndr_unpack
        from samba.ntacls import dsacl2fsacl
        guid = "{"+str(uuid.uuid5(uuid.NAMESPACE_URL, self.config["connection_id"]+":"+operation)).upper()+"}"
        dn = "CN="+guid+","+self.policy_base
        self.db.add({"dn": dn, "objectClass": "groupPolicyContainer"})
        for part in ("User", "Machine"):
            self.db.add({"dn": "CN="+part+","+dn, "objectClass": "container"})
        row = self.search(base=dn, scope=self.ldb.SCOPE_BASE)[0]
        self.modify(row, {"displayName": name, "gPCFileSysPath": "\\\\%s\\sysvol\\%s\\Policies\\%s" % (self.realm, self.realm, guid), "versionNumber": "0", "flags": "0", "gPCFunctionalityVersion": "2"})
        row = self.search(base=dn, scope=self.ldb.SCOPE_BASE)[0]
        create_directory_hier(self.smb, self.realm+"\\Policies\\"+guid)
        sd = ndr_unpack(security.descriptor, bytes(row["nTSecurityDescriptor"][0]))
        fs = dsacl2fsacl(sd.as_sddl(), security.dom_sid(self.db.get_domain_sid()), as_sddl=False)
        self.smb.set_acl(self.realm+"\\Policies\\"+guid, fs, security.SECINFO_DACL | security.SECINFO_PROTECTED_DACL)
        self.write_file(guid, "GPT.INI", b"[General]\r\nVersion=0\r\n")
        atomic_json(self.state / ("created-"+guid.strip("{}")+".json"), {"operation": operation, "guid": guid})
        return row

    def publish(self, row, p):
        from samba.dcerpc import preg
        from samba.ndr import ndr_unpack, ndr_pack
        guid = self.attr(row, "name")
        manifest = self.state / ("policy-"+self.guid(row)+".json")
        saved = json.loads(manifest.read_text()) if manifest.exists() else {}
        definitions = {saved["scope"]: saved} if "scope" in saved else saved
        old = definitions.get(p["scope"], {})
        files = self.files(guid)
        section = "Machine" if p["scope"] == "computer" else "User"
        path = section+"\\Registry.pol"
        # Windows paths are case-insensitive even if the dictionary is not.
        path = next((f for f in files if f.lower() == path.lower()), path)
        pol = ndr_unpack(preg.file, base64.b64decode(files[path])) if path in files else preg.file()
        desired = registry_values(p)
        owned = {(k.lower(), n.lower()) for k,n,_,_ in registry_values(old)} if old else set()
        replaced = owned | {(k.lower(), n.lower()) for k,n,_,_ in desired}
        entries = [e for e in (pol.entries or []) if (e.keyname.lower(), e.valuename.lower()) not in replaced]
        for key, name, typ, value in desired:
            entry = preg.entry()
            entry.keyname, entry.valuename, entry.type = key, name, typ
            entry.data = value
            entries.append(entry)
        pol.num_entries, pol.entries = len(entries), entries
        self.write_file(guid, path, ndr_pack(pol))
        extension = self.attr(row, "gPCMachineExtensionNames" if section == "Machine" else "gPCUserExtensionNames")
        if desired and REG_CSE.lower() not in extension.lower():
            extension += REG_CSE
        if section == "User":
            for kind, (folder, _, _, _, ext) in GPP.items():
                relative = "User\\Preferences\\%s\\%s.xml" % (folder, folder)
                relative = next((f for f in files if f.lower() == relative.lower()), relative)
                prior = base64.b64decode(files[relative]) if relative in files else None
                owned_ids = [v["id"] for v in old.get(kind, [])]
                if p.get(kind) or owned_ids:
                    self.write_file(guid, relative, merge_preferences(prior, kind, p.get(kind, []), owned_ids))
                    if ext.lower() not in extension.lower():
                        extension += ext
        version = int(self.attr(row, "versionNumber", "0"))
        version = ((((version >> 16)+1)&65535)<<16)|(version&65535) if section == "User" else (version&0xffff0000)|((version+1)&65535)
        ini_path = next((f for f in files if f.lower() == "gpt.ini"), "GPT.INI")
        ini = configparser.ConfigParser()
        ini.optionxform = str
        if ini_path in files:
            ini.read_string(base64.b64decode(files[ini_path]).decode("utf-8-sig"))
        if not ini.has_section("General"):
            ini.add_section("General")
        ini["General"]["Version"] = str(version)
        out = io.StringIO(); ini.write(out)
        self.write_file(guid, ini_path, out.getvalue().replace("\n", "\r\n").encode())
        self.modify(row, {"displayName": p["name"], "versionNumber": str(version), "gPC"+section+"ExtensionNames": extension})
        definitions[p["scope"]] = p
        atomic_json(manifest, definitions)

    def apply(self, payload):
        action = payload["action"]
        require(action.get("kind") in KINDS)
        require(self.mappings is None or action["kind"] in GPO_KINDS, "DIRECTORY_ACTION_UNSUPPORTED")
        before = payload["before"]["objects"]
        current = self.inventory()["objects"]
        refs = set()
        if action.get("objectId"):
            refs.add(action["objectId"])
        if action.get("parentId"):
            refs.add(action["parentId"])
        if action.get("filterGroupId"):
            refs.add(action["filterGroupId"])
        refs.update(action.get("groupIds", []))
        refs.update(action.get("gpoIds", []))
        refs.update(action.get("memberIds", []))
        if action["kind"] == "gpo.publish":
            p = validate_policy(action["definition"], settings_only=bool(action.get("objectId")))
            if not action.get("objectId"):
                refs.update([p["targetOuId"], p["filterGroupId"]])
            if p.get("computerTargetId"):
                refs.add(p["computerTargetId"])
        resolved = {}
        for ident in refs:
            prior = next((o for o in before if o["id"] == ident), None)
            require(prior, "DIRECTORY_OBJECT_MISSING")
            expected = prior["properties"].get("windowsIdentity", prior)
            live = next((o for o in current if o["id"] == expected["id"]), None)
            require(live and live["version"] == expected["version"], "DIRECTORY_VERSION_CHANGED")
            require(live["kind"] == "gpo" or self.allowed_dn(live["dn"]), "DIRECTORY_WRITE_OUTSIDE_SCOPE")
            resolved[ident] = self.row(live["id"])
        def is_kind(ident, kind):
            obj = next((o for o in current if o["id"] == self.guid(resolved[ident])), None)
            require(obj and obj["kind"] == kind, "DIRECTORY_TARGET_INVALID")
        if action["kind"] in ("gpo.order", "gpo.inheritance", "ou.update"):
            is_kind(action["objectId"], "ou")
        elif action["kind"] in GPO_KINDS and action.get("objectId"):
            is_kind(action["objectId"], "gpo")
        elif action["kind"].startswith("account.") and action.get("objectId"):
            is_kind(action["objectId"], "account")
        elif action["kind"] == "group.members":
            is_kind(action["objectId"], "group")
            for ident in action.get("memberIds", []):
                require(next(o for o in current if o["id"] == self.guid(resolved[ident]))["kind"] in ("account", "group", "computer"), "DIRECTORY_TARGET_INVALID")
        elif action["kind"] == "object.move":
            require(next(o for o in current if o["id"] == self.guid(resolved[action["objectId"]]))["kind"] in ("account", "group", "computer", "ou"), "DIRECTORY_TARGET_INVALID")
        if action.get("parentId"): is_kind(action["parentId"], "ou")
        for ident in action.get("groupIds", []): is_kind(ident, "group")
        for ident in action.get("gpoIds", []): is_kind(ident, "gpo")
        if action.get("filterGroupId"): is_kind(action["filterGroupId"], "group")
        if action["kind"] == "gpo.publish" and not action.get("objectId"):
            is_kind(p["targetOuId"], "ou"); is_kind(p["filterGroupId"], "group")
        if action["kind"] == "gpo.publish" and p.get("computerTargetId"):
            computer = next(o for o in current if o["id"] == self.guid(resolved[p["computerTargetId"]]))
            require(computer["kind"] in ("ou", "group"), "DIRECTORY_TARGET_INVALID")
        target = resolved.get(action.get("objectId"))
        if target is not None and action["kind"] in ("gpo.publish", "gpo.filter", "gpo.link"):
            require(self.gpo_allowed(self.attr(target, "name")), "DIRECTORY_GPO_OUTSIDE_SCOPE")
        backup = {ident: self.snapshot(row, "groupPolicyContainer" in [str(x) for x in row["objectClass"]]) for ident,row in resolved.items()}
        backup_path = self.state / ("backup-"+fingerprint(payload["operationId"])+".json")
        atomic_json(backup_path, backup)
        self.db.transaction_start()
        try:
            # Recheck after taking the directory transaction lock and before the first mutation.
            for ident,row in resolved.items():
                require(fingerprint(self.snapshot(self.row(self.guid(row)), "groupPolicyContainer" in [str(x) for x in row["objectClass"]])) == fingerprint(backup[ident]), "DIRECTORY_VERSION_CHANGED")
            kind = action["kind"]
            if kind in DIRECTORY_KINDS:
                target = self.apply_directory(action, payload["operationId"], resolved)
            elif kind == "gpo.publish":
                target = target if target is not None else self.create(payload["operationId"], p["name"])
                self.publish(target, p)
                if not action.get("objectId"):
                    self.filter(target, [resolved[p["filterGroupId"]]])
                    ou = resolved[p["targetOuId"]]
                    links = parse_links(self.attr(ou, "gPLink"))
                    if not any(l["dn"].lower() == str(target.dn).lower() for l in links):
                        links.insert(0, {"dn": str(target.dn), "options": 0})
                    self.modify(ou, {"gPLink": encode_links(links)})
            elif kind == "gpo.filter":
                represented_sids = {g["properties"].get("sid") for g in current if g["kind"] == "group"}
                self.filter(target, [resolved[x] for x in action["groupIds"]], represented_sids)
            elif kind == "gpo.inheritance":
                require(type(action.get("blocked")) is bool)
                value = int(self.attr(target, "gPOptions", "0"))
                self.modify(target, {"gPOptions": str(value | 1 if action["blocked"] else value & ~1)})
            elif kind == "gpo.order":
                links = parse_links(self.attr(target, "gPLink"))
                ordered = [str(resolved[x].dn) for x in action["gpoIds"]]
                require(len(ordered) == len(links) == len(set(d.lower() for d in ordered)) and {d.lower() for d in ordered} == {l["dn"].lower() for l in links}, "DIRECTORY_LINKS_CHANGED")
                self.modify(target, {"gPLink": encode_links([next(l for l in links if l["dn"].lower() == dn.lower()) for dn in ordered])})
            elif kind == "gpo.link":
                ou = resolved[action["parentId"]]
                old = parse_links(self.attr(ou, "gPLink"))
                links = [l for l in old if l["dn"].lower() != str(target.dn).lower()]
                if not action.get("remove", False):
                    options = (0 if action.get("enabled", True) else 1) | (2 if action.get("enforced", False) else 0)
                    previous = next((i for i,l in enumerate(old) if l["dn"].lower() == str(target.dn).lower()), 0)
                    order = action.get("order", previous+1)
                    require(type(order) is int and 1 <= order <= len(links)+1)
                    links.insert(order-1, {"dn": str(target.dn), "options": options})
                self.modify(ou, {"gPLink": encode_links(links)})
                if action.get("filterGroupId"):
                    self.filter(target, [resolved[action["filterGroupId"]]])
            self.db.transaction_commit()
        except Exception:
            self.db.transaction_cancel()
            # SYSVOL is not transactional. Never silently retry a possibly partial mutation.
            raise Conflict("DIRECTORY_PARTIAL_UNCERTAIN")
        for guid, values in self.expected_attributes.items():
            observed = self.row(guid)
            for key, expected in values.items():
                actual = self.attrs(observed, key)
                wanted = list(expected) if isinstance(expected, (list, tuple)) else [expected]
                normalize = lambda values: sorted(bytes(v) if isinstance(v, bytes) else str(v) for v in values)
                require(normalize(actual) == normalize(wanted), "DIRECTORY_READBACK_FAILED")
        for path, expected in self.expected_files.items():
            require(self.smb.loadfile(path) == expected, "DIRECTORY_READBACK_FAILED")
        for saved in backup.values():
            if "files" not in saved:
                continue
            name = base64.b64decode(saved["attributes"]["name"][0]).decode()
            prefix = self.realm + "\\Policies\\" + name + "\\"
            expected_names = set(saved["files"]) | {path[len(prefix):] for path in self.expected_files if path.startswith(prefix)}
            require(set(self.files(name)) == expected_names, "DIRECTORY_CONCURRENT_SYSVOL_EDIT")
            for relative, value in saved["files"].items():
                path = self.realm + "\\Policies\\" + name + "\\" + relative
                require(self.smb.loadfile(path) == self.expected_files.get(path, base64.b64decode(value)), "DIRECTORY_CONCURRENT_SYSVOL_EDIT")
        readback = self.inventory()
        live_target = self.row(self.guid(target)) if target is not None else None
        return {"verified": True, "endpointConfirmed": False, "objectId": self.guid(live_target) if live_target is not None else None, "version": fingerprint(self.snapshot(live_target)) if live_target is not None else None, "dn": str(live_target.dn) if live_target is not None else None, "inventoryFingerprint": fingerprint(readback["objects"]), "backupId": backup_path.name}

class Runner:
    def __init__(self, config, factory=SambaAdapter):
        self.config, self.factory = config, factory
        self.state = Path(config["state_directory"])
        self.state.mkdir(parents=True, exist_ok=True, mode=0o700)
        require(self.state.stat().st_mode & 0o077 == 0, "DIRECTORY_STATE_PERMISSIONS")

    def execute(self, route, payload):
        require(payload.get("connectionId") in self.config.get("connection_ids", [self.config["connection_id"]]), "DIRECTORY_CONNECTION_FORBIDDEN")
        require(route in ("/inventory", "/apply"))
        with open(self.state / "runner.lock", "a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            adapter = self.factory(self.config)
            if route == "/inventory":
                return adapter.inventory()
            require(isinstance(payload.get("operationId"), str) and 8 <= len(payload["operationId"]) <= 220)
            require(payload.get("action", {}).get("kind") in KINDS)
            if payload["action"]["kind"] == "gpo.publish":
                validate_policy(payload["action"]["definition"])
            path = self.state / ("operation-" + fingerprint(payload["operationId"]) + ".json")
            digest = fingerprint(payload)
            if path.exists():
                journal = json.loads(path.read_text())
                require(journal["requestHash"] == digest, "DIRECTORY_IDEMPOTENCY_CONFLICT")
                require(journal["status"] == "completed", "DIRECTORY_PARTIAL_UNCERTAIN")
                return journal["result"]
            journal = {"requestHash": digest, "status": "started", "startedAt": int(time.time()*1000)}
            atomic_json(path, journal)
            try:
                result = adapter.apply(payload)
                require(result.get("verified") is True, "DIRECTORY_READBACK_FAILED")
                journal.update(status="completed", result=result)
                atomic_json(path, journal)
                return result
            except Exception as error:
                journal.update(status="blocked", error=error.code if isinstance(error, Conflict) else "DIRECTORY_PARTIAL_UNCERTAIN")
                atomic_json(path, journal)
                raise Conflict(journal["error"])

class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        token = Path(self.server.config["token_file"]).read_text().strip()
        if not hmac.compare_digest(self.headers.get("Authorization", ""), "Bearer " + token):
            self.respond(403, {"error": {"code": "DIRECTORY_TOKEN_INVALID"}}); return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            require(0 < length <= 16_777_216)
            self.connection.settimeout(30)
            payload = json.loads(self.rfile.read(length))
            result = self.server.runner.execute(self.path, payload)
            self.respond(200, result)
        except (Conflict, ValueError, KeyError, TypeError) as error:
            self.respond(409, {"error": {"code": error.code if isinstance(error, Conflict) else "DIRECTORY_ACTION_INVALID"}})
        except Exception:
            self.respond(503, {"error": {"code": "DIRECTORY_UNAVAILABLE"}})
    def respond(self, status, value):
        body = json.dumps(value).encode()
        self.send_response(status); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self, *args):
        pass  # Never log tokens, object data or provider exceptions.

def main():
    parser = argparse.ArgumentParser(); parser.add_argument("--config", required=True); args = parser.parse_args()
    config_path = Path(args.config)
    require(config_path.stat().st_mode & 0o077 == 0, "DIRECTORY_CONFIG_PERMISSIONS")
    config = json.loads(config_path.read_text())
    for field in ("token_file", "credentials_file", "identity_mapping_file"):
        if config.get(field):
            require(Path(config[field]).stat().st_mode & 0o077 == 0, "DIRECTORY_CONFIG_PERMISSIONS")
    require(len(Path(config["token_file"]).read_text().strip()) >= 32)
    server = http.server.HTTPServer((config.get("listen_host", "127.0.0.1"), config.get("listen_port", 18444)), Handler)
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); tls.minimum_version = ssl.TLSVersion.TLSv1_2
    tls.load_cert_chain(config["certificate_file"], config["private_key_file"])
    server.socket = tls.wrap_socket(server.socket, server_side=True)
    server.config, server.runner = config, Runner(config)
    server.serve_forever()

if __name__ == "__main__":
    main()
