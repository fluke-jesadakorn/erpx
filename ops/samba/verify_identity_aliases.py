#!/usr/bin/env python3
import argparse
import json
import os

from samba.auth import system_session
from samba.param import LoadParm
from samba.samdb import SamDB


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    lp = LoadParm()
    lp.load(args.config)
    db = SamDB(lp=lp, session_info=system_session())
    rows = json.load(open(args.source, encoding="utf-8"))
    aliases = {}
    missing_types = set()
    for row in rows:
        found = db.search(
            expression="(objectGUID=%s)" % row["sambaGuid"],
            attrs=["distinguishedName"],
        )
        if len(found) == 1:
            aliases[row["sambaGuid"]] = row["udmUuid"]
        else:
            missing_types.add(row["udmModule"])
    if len(set(aliases.values())) != len(aliases):
        raise RuntimeError("Verified aliases are not one-to-one")
    with open(args.output, "w", encoding="utf-8") as stream:
        json.dump(aliases, stream, sort_keys=True)
    os.chmod(args.output, 0o600)
    print("verified", len(aliases), "missing_after_restore", len(rows) - len(aliases))
    print("missing_types", sorted(missing_types))


if __name__ == "__main__":
    main()
