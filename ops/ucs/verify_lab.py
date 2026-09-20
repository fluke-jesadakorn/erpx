#!/usr/bin/env python3
"""Opt-in destructive-to-own-fixtures verification. Run only on an isolated UCS lab."""
import argparse
import base64
import json
from pathlib import Path
import uuid
from gpo_runner import Runner, SambaAdapter, Conflict


def verify(config, target_dn, group_name):
    from samba.ndr import ndr_pack, ndr_unpack
    from samba.dcerpc import preg
    import xml.etree.ElementTree as ET

    runner = Runner(config)
    connection = config['connection_id']
    def inventory():
        return runner.execute('/inventory', {'connectionId': connection})
    def payload(action, before=None):
        return {'connectionId': connection, 'operationId': str(uuid.uuid4()), 'action': action, 'before': before or inventory()}
    def apply(action, before=None):
        result = runner.execute('/apply', payload(action, before))
        assert result['verified'] and result['endpointConfirmed'] is False
        return result
    before = inventory()
    ou = next(o for o in before['objects'] if o['dn'].lower() == target_dn.lower() and o['kind'] == 'ou')
    group = next(o for o in before['objects'] if o['name'] == group_name and o['kind'] == 'group')
    extra_group = next(o for o in before['objects'] if o['kind'] == 'group' and o['id'] != group['id'])
    assert 'lab' in ou['dn'].lower(), 'Use an isolated lab OU'
    initial_block = ou['properties']['blockInheritance']
    name = 'ERPX-Verification-' + uuid.uuid4().hex[:10]
    recipe = {'id': str(uuid.uuid4()), 'name': name, 'scope': 'user', 'usb': 'deny', 'lockSeconds': 600,
              'wallpaper': r'\\ucs.erpx.test\netlogon\verification.jpg', 'wallpaperStyle': '10',
              'targetOuId': ou['id'], 'filterGroupId': group['id'], 'computerTargetId': None,
              'drives': [{'id': str(uuid.uuid4()), 'letter': 'V', 'label': 'Verification', 'path': r'\\ucs.erpx.test\lab', 'remove': False}],
              'printers': [{'id': str(uuid.uuid4()), 'path': r'\\ucs.erpx.test\LabPrinter', 'isDefault': False, 'remove': False}]}
    request = payload({'kind': 'gpo.publish', 'objectId': None, 'definition': recipe}, before)
    result = runner.execute('/apply', request)
    assert runner.execute('/apply', request) == result
    ident = result['objectId']
    adapter = SambaAdapter(config)
    row = adapter.row(ident)
    cn = adapter.attr(row, 'name')
    files = adapter.files(cn)
    pol = ndr_unpack(preg.file, base64.b64decode(files['User\\Registry.pol']))
    entry = preg.entry(); entry.keyname = r'Software\ERPXVerification'; entry.valuename = 'Unmanaged'; entry.type = 1; entry.data = 'retain'
    entries = list(pol.entries) + [entry]; pol.num_entries = len(entries); pol.entries = entries
    adapter.write_file(cn, 'User\\Registry.pol', ndr_pack(pol))
    adapter.write_file(cn, 'User\\Unmanaged.dat', b'retain')
    drives = ET.fromstring(base64.b64decode(files['User\\Preferences\\Drives\\Drives.xml']))
    drives.find('Drive').set('unmanaged', 'retain')
    ET.SubElement(drives.find('Drive'), 'Filters', {'unmanaged': 'retain'})
    adapter.write_file(cn, 'User\\Preferences\\Drives\\Drives.xml', ET.tostring(drives))
    apply({'kind': 'gpo.filter', 'objectId': ident, 'groupIds': [group['id'], extra_group['id']]})
    apply({'kind': 'gpo.link', 'objectId': ident, 'parentId': ou['id'], 'remove': True})
    current = inventory()
    update = {'kind': 'gpo.publish', 'objectId': ident, 'definition': {**recipe, 'lockSeconds': 1200}}
    apply(update, current)
    patched = next(o for o in inventory()['objects'] if o['id'] == ident)
    assert not patched['memberIds'], 'Settings patch unexpectedly linked an existing GPO'
    assert set(patched['properties']['securityFilterGroupIds']) == {group['id'], extra_group['id']}, 'Settings patch changed security filtering'
    try:
        apply(update, current)
        raise AssertionError('Stale version accepted')
    except Conflict as error:
        assert error.code == 'DIRECTORY_VERSION_CHANGED'
    adapter = SambaAdapter(config)
    files = adapter.files(cn)
    pol = ndr_unpack(preg.file, base64.b64decode(files['User\\Registry.pol']))
    assert any(e.valuename == 'Unmanaged' and e.data == 'retain' for e in pol.entries)
    assert any(e.valuename == 'ScreenSaveTimeOut' and e.data == '1200' for e in pol.entries)
    assert base64.b64decode(files['User\\Unmanaged.dat']) == b'retain'
    drives = ET.fromstring(base64.b64decode(files['User\\Preferences\\Drives\\Drives.xml']))
    assert drives.find('Drive').get('unmanaged') == 'retain'
    assert drives.find('Drive/Filters').get('unmanaged') == 'retain'
    assert ET.fromstring(base64.b64decode(files['User\\Preferences\\Printers\\Printers.xml'])).find('SharedPrinter/Properties').get('path') == recipe['printers'][0]['path']
    apply({'kind': 'gpo.filter', 'objectId': ident, 'groupIds': [group['id']]})
    assert next(o for o in inventory()['objects'] if o['id'] == ident)['properties']['securityFilterGroupIds'] == [group['id']]
    computer = {**recipe, 'id': str(uuid.uuid4()), 'name': name+'-Computer', 'scope': 'computer', 'computerTargetId': ou['id'], 'wallpaper': None, 'drives': [], 'printers': []}
    computer_id = apply({'kind': 'gpo.publish', 'objectId': None, 'definition': computer})['objectId']
    adapter = SambaAdapter(config)
    machine = adapter.files(adapter.attr(adapter.row(computer_id), 'name'))
    assert any(e.valuename == 'InactivityTimeoutSecs' and e.data == 600 for e in ndr_unpack(preg.file, base64.b64decode(machine['Machine\\Registry.pol'])).entries)
    apply({'kind': 'gpo.link', 'objectId': ident, 'parentId': ou['id'], 'enabled': False, 'enforced': True, 'order': 1})
    linked = next(o for o in inventory()['objects'] if o['id'] == ou['id'])['properties']['gpoLinks']
    assert linked[0] == {'gpoId': ident, 'enabled': False, 'enforced': True}
    apply({'kind': 'gpo.order', 'objectId': ou['id'], 'gpoIds': [l['gpoId'] for l in reversed(linked)]})
    apply({'kind': 'gpo.inheritance', 'objectId': ou['id'], 'blocked': not initial_block})
    assert next(o for o in inventory()['objects'] if o['id'] == ou['id'])['properties']['blockInheritance'] is not initial_block

    class LostReadback(SambaAdapter):
        def inventory(self):
            self.reads = getattr(self, 'reads', 0) + 1
            if self.reads == 2:
                raise RuntimeError('Injected post-commit connection loss')
            return super().inventory()
    uncertain = payload({'kind': 'gpo.link', 'objectId': ident, 'parentId': ou['id'], 'enabled': True, 'enforced': False})
    for attempt in (Runner(config, LostReadback), runner):
        try:
            attempt.execute('/apply', uncertain)
            raise AssertionError('Uncertain write replayed')
        except Conflict as error:
            assert error.code == 'DIRECTORY_PARTIAL_UNCERTAIN'
    # Fresh previews undo only this run's test links and restore the original inheritance flag.
    for test_id in (ident, computer_id):
        apply({'kind': 'gpo.link', 'objectId': test_id, 'parentId': ou['id'], 'remove': True})
    apply({'kind': 'gpo.inheritance', 'objectId': ou['id'], 'blocked': initial_block})
    print('PASS: user/computer settings, Drive/Printer, unknown-value preservation, filtering, link flags/order, inheritance, replay, stale version, real partial write recovery; endpoint confirmation remains false')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', required=True)
    parser.add_argument('--target-ou', required=True)
    parser.add_argument('--filter-group', required=True)
    args = parser.parse_args()
    verify(json.loads(Path(args.config).read_text()), args.target_ou, args.filter_group)
