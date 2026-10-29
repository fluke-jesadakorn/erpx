# Directory LDAP dependency notices

The optional native Directory LDAPS adapter uses unmodified `ldap3_proto 0.8.1`
under MPL-2.0 and its unmodified `ldap3_lber 0.8.1` dependency under MIT.
ERPX does not relicense either dependency under Apache-2.0. The exact retained
license files and registry/source checksums are recorded in
[directory-ldap-provenance.json](directory-ldap-provenance.json).

The source for these upstream components is available from the pinned
[upstream commit](https://github.com/kanidm/ldap3/tree/a41de11d4331bb8977b7bc373aba78bd153674ea)
and the exact registry archive URLs recorded in the provenance file. A binary
release that includes the optional adapter must distribute these notices and
inform recipients how to obtain the corresponding MPL source. These notices
cover these components only; the complete transitive release inventory and
production acceptance remain pending.
