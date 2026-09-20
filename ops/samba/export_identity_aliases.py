#!/usr/bin/env python3
"""Export connector-proven UCS entryUUID -> Samba objectGUID evidence.

Run on the UCS source DC. The output contains identifiers and DNs but no
passwords. ERPX consumes the inverse GUID -> UUID map only after the restored
directory has independently confirmed each GUID/DN pair.
"""
import json
import sqlite3
import sys

import univention.admin.uldap
from samba.auth import system_session
from samba.dcerpc.misc import GUID
from samba.ndr import ndr_unpack
from samba.param import LoadParm
from samba.samdb import SamDB
from ldb import LdbError


def main():
    lp = LoadParm()
    lp.load_default()
    sam = SamDB(lp=lp, session_info=system_session())
    base = str(sam.get_default_basedn())
    lo, _ = univention.admin.uldap.getAdminConnection()
    ucs = lo.search(
        base=base,
        attr=["entryUUID", "univentionObjectType", "sambaSID"],
    )
    store = sqlite3.connect("/etc/univention/connector/s4internal.sqlite")
    dn_map = dict(store.execute('SELECT Key,Value FROM "DN Mapping UCS"').fetchall())
    result = []
    for dn, attrs in ucs:
        entry_uuid = attrs.get("entryUUID")
        mapped_dn = dn_map.get(dn.lower())
        if not entry_uuid or not mapped_dn:
            continue
        try:
            rows = sam.search(base=mapped_dn, scope=0, attrs=["objectGUID"])
        except LdbError as error:
            if error.args and error.args[0] == 32:
                continue
            raise
        if len(rows) != 1:
            raise RuntimeError("Mapped AD object missing or ambiguous: " + dn)
        result.append(
            {
                "sambaGuid": str(ndr_unpack(GUID, bytes(rows[0]["objectGUID"][0]))).lower(),
                "udmUuid": entry_uuid[0].decode().lower(),
                "udmDn": dn,
                "sambaDn": mapped_dn,
                "udmModule": (attrs.get("univentionObjectType") or [b""])[0].decode(),
            }
        )
    if len({row["sambaGuid"] for row in result}) != len(result):
        raise RuntimeError("Duplicate Samba GUID in connector evidence")
    if len({row["udmUuid"] for row in result}) != len(result):
        raise RuntimeError("Duplicate UCS entryUUID in connector evidence")
    json.dump(sorted(result, key=lambda row: row["sambaDn"].lower()), sys.stdout)


if __name__ == "__main__":
    main()
