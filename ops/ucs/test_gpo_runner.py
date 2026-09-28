import copy
import json
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET
from gpo_runner import Runner, Conflict, APPLY_GUID, validate_policy, registry_values, merge_preferences, parse_links, encode_links, remove_represented_apply_aces

class RunnerTests(unittest.TestCase):
    def test_finite_settings_and_explicit_computers(self):
        policy = dict(name="Lab policy",scope="computer",usb="deny",targetOuId="ou",filterGroupId="group",computerTargetId="ou",lockSeconds=600)
        self.assertEqual(len(registry_values(validate_policy(policy))),2)
        for patch in ({"computerTargetId":None},{"wallpaper":r"C:\test.jpg"},{"usb":"arbitrary"},{"lockSeconds":True}):
            with self.assertRaises(Conflict): validate_policy({**policy,**patch})

    def test_preferences_preserve_unmanaged_nodes(self):
        uid="aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
        original=b'<Drives><Drive uid="{bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb}" unknown="retain"><Properties action="U" letter="T"/></Drive><Extension untouched="yes"/></Drives>'
        desired=[dict(id=uid,letter="S",path=r"\\server\share",label="A & B",remove=False)]
        result=merge_preferences(original,"drives",desired,[])
        root=ET.fromstring(result)
        self.assertEqual(len(root.findall('Drive')),2)
        self.assertEqual(root.find('Extension').get('untouched'),'yes')
        self.assertEqual(root.findall('Drive')[1].find('Properties').get('label'),'A & B')
        cleaned=ET.fromstring(merge_preferences(result,'drives',[],[uid]))
        self.assertEqual(len(cleaned.findall('Drive')),1)
        self.assertEqual(cleaned.find('Drive').get('unknown'),'retain')

    def test_link_order_and_flags_round_trip(self):
        value='[LDAP://CN={A},CN=Policies,DC=test;2][LDAP://CN={B},CN=Policies,DC=test;1]'
        self.assertEqual(encode_links(parse_links(value)),value)
        self.assertTrue(parse_links(value)[0]["dn"].startswith("CN={B}"))  # GPMC order 1 is the rightmost LDAP link.
        with self.assertRaises(Conflict): parse_links(value+'junk')

    def test_filter_update_preserves_unrepresented_apply_aces(self):
        managed='S-1-5-21-100'
        unmanaged='S-1-5-21-200'
        managed_ace='(OA;;CR;%s;;%s)' % (APPLY_GUID,managed)
        unmanaged_ace='(OA;;CR;%s;;%s)' % (APPLY_GUID,unmanaged)
        read_ace='(A;;RPLCRC;;;%s)' % managed
        sddl='O:SYG:SYD:'+managed_ace+unmanaged_ace+read_ace
        updated=remove_represented_apply_aces(sddl,{managed})
        self.assertNotIn(managed_ace,updated)
        self.assertIn(unmanaged_ace,updated)
        self.assertIn(read_ace,updated)
        self.assertNotIn(unmanaged_ace,remove_represented_apply_aces(sddl))

    def test_journal_idempotency_and_uncertain_partial_failure(self):
        class Adapter:
            calls=0
            def __init__(self,config): pass
            def apply(self,payload):
                Adapter.calls+=1
                if payload['action'].get('fail'): raise RuntimeError('transport failed after write')
                return {'verified':True,'endpointConfirmed':False}
        with tempfile.TemporaryDirectory() as directory:
            runner=Runner({'connection_id':'lab','state_directory':directory},Adapter)
            payload={'connectionId':'lab','operationId':'operation-1','action':{'kind':'gpo.inheritance','objectId':'ou','blocked':True},'before':{'objects':[]}}
            first=runner.execute('/apply',payload)
            self.assertEqual(runner.execute('/apply',payload),first)
            self.assertEqual(Adapter.calls,1)
            other=copy.deepcopy(payload);other['action']['blocked']=False
            with self.assertRaisesRegex(Conflict,'IDEMPOTENCY_CONFLICT'):runner.execute('/apply',other)
            failed={**payload,'operationId':'operation-2','action':{**payload['action'],'fail':True}}
            with self.assertRaisesRegex(Conflict,'PARTIAL_UNCERTAIN'):runner.execute('/apply',failed)
            with self.assertRaisesRegex(Conflict,'PARTIAL_UNCERTAIN'):runner.execute('/apply',failed)
            self.assertEqual(Adapter.calls,2)
            with self.assertRaisesRegex(Conflict,'CONNECTION_FORBIDDEN'):runner.execute('/apply',{**payload,'connectionId':'wrong'})
            for journal in Path(directory).glob('operation-*.json'):
                self.assertEqual(journal.stat().st_mode&0o777,0o600)

    def test_native_directory_actions_use_the_same_finite_journal(self):
        class Adapter:
            calls=0
            def __init__(self,config): pass
            def apply(self,payload):
                Adapter.calls+=1
                return {'verified':True,'objectId':'guid','dn':'CN=User,OU=Lab,DC=test','version':'2'}
        with tempfile.TemporaryDirectory() as directory:
            runner=Runner({'connection_id':'native','state_directory':directory},Adapter)
            payload={'connectionId':'native','operationId':'native-operation-1','action':{'kind':'account.create','username':'user1','firstName':'Test','lastName':'User','parentId':'ou'},'before':{'objects':[]}}
            result=runner.execute('/apply',payload)
            self.assertEqual(result['objectId'],'guid')
            self.assertEqual(runner.execute('/apply',payload),result)
            self.assertEqual(Adapter.calls,1)

if __name__=='__main__': unittest.main()
