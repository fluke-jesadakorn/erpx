#requires -Version 5.1
<#
Run under a dedicated domain service identity with RSAT Group Policy tools.
The HTTPS binding and certificate are installed separately by the machine owner.
Config: ConnectionId, Prefix, TokenFile, Domain, Server, LdapPort (636),
BaseDn, WriteBaseDns, StateDirectory. Never disable TLS certificate validation.
#>
param([Parameter(Mandatory=$true)][string]$ConfigPath)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.DirectoryServices.Protocols
Import-Module GroupPolicy -ErrorAction Stop
$config = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
if (-not $config.Prefix.StartsWith('https://')) { throw 'HTTPS_REQUIRED' }
$token = (Get-Content -LiteralPath $config.TokenFile -Raw).Trim()
if ($token.Length -lt 32) { throw 'TOKEN_TOO_SHORT' }
New-Item -ItemType Directory -Path $config.StateDirectory -Force | Out-Null
$script:ldap = New-Object System.DirectoryServices.Protocols.LdapConnection(
  (New-Object System.DirectoryServices.Protocols.LdapDirectoryIdentifier($config.Server,[int]$config.LdapPort)))
$ldap.AuthType = [System.DirectoryServices.Protocols.AuthType]::Negotiate
$ldap.SessionOptions.ProtocolVersion = 3
$ldap.SessionOptions.SecureSocketLayer = $true
$ldap.Timeout = [TimeSpan]::FromSeconds(20)
$ldap.Bind()
function Values($entry,[string]$name) {
  if (-not $entry.Attributes.Contains($name)) { return @() }
  return @($entry.Attributes[$name].GetValues([string]))
}
function Search-Ldap([string]$base,[string]$filter,[string]$scope='Subtree') {
  $request=New-Object System.DirectoryServices.Protocols.SearchRequest($base,$filter,[System.DirectoryServices.Protocols.SearchScope]::$scope,@('objectGUID','entryUUID','cn','name','sAMAccountName','objectClass','uSNChanged','member','description','userAccountControl','givenName','sn','displayName','mail','gPCFileSysPath','versionNumber','gPLink','gPCUserExtensionNames'))
  $page=New-Object System.DirectoryServices.Protocols.PageResultRequestControl(500)
  $request.Controls.Add($page)|Out-Null
  $result=New-Object System.Collections.Generic.List[object]
  do {
    $response=[System.DirectoryServices.Protocols.SearchResponse]$ldap.SendRequest($request)
    if ($response.References.Count -gt 0) { throw 'DIRECTORY_REFERRAL_UNSUPPORTED' }
    foreach($entry in $response.Entries){$result.Add($entry)}
    $control=@($response.Controls|Where-Object {$_ -is [System.DirectoryServices.Protocols.PageResultResponseControl]})
    $page.Cookie=if($control.Count){$control[0].Cookie}else{[byte[]]@()}
    if($result.Count -gt 10000){throw 'DIRECTORY_LIMIT_EXCEEDED'}
  } while($page.Cookie.Length)
  return $result.ToArray()
}
function Read-Object([string]$dn) {
  $rows=@(Search-Ldap $dn '(objectClass=*)' 'Base')
  if($rows.Count -ne 1){throw 'DIRECTORY_OBJECT_MISSING'}
  return $rows[0]
}
function Guid-Of($entry){
  if(@(Values $entry 'objectClass') -contains 'groupPolicyContainer'){return ([Guid]::Parse((@(Values $entry 'cn') -join '').Trim('{}'))).ToString()}
  return (New-Object Guid(,$entry.Attributes['objectGUID'][0])).ToString()
}
function Gpo-Version([string]$id){
  $report=Get-GPOReport -Guid $id -ReportType Xml -Domain $config.Domain -Server $config.Server
  $xml=New-Object Xml.XmlDocument;$xml.XmlResolver=$null;$xml.LoadXml($report)
  foreach($node in @($xml.SelectNodes('//*[local-name()="ReadTime"]'))){$node.ParentNode.RemoveChild($node)|Out-Null}
  return ([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($xml.OuterXml)))).Replace('-','')
}
function Assert-Boundary([string]$dn) {
  $found=$false
  foreach($base in $config.WriteBaseDns){if($dn.Equals($base,[StringComparison]::OrdinalIgnoreCase)-or$dn.EndsWith(','+$base,[StringComparison]::OrdinalIgnoreCase)){$found=$true}}
  if(-not $found){throw 'DIRECTORY_WRITE_OUTSIDE_SCOPE'}
}
function Inventory {
  $rows=@(Search-Ldap $config.BaseDn '(|(objectClass=user)(objectClass=group)(objectClass=organizationalUnit)(objectClass=container)(objectClass=groupPolicyContainer))')
  $objects=@();$byDn=@{}
  foreach($r in $rows){$byDn[$r.DistinguishedName.ToLowerInvariant()]=Guid-Of $r}
  foreach($r in $rows){
    $classes=Values $r 'objectClass'
    $kind=if($classes -contains 'groupPolicyContainer'){'gpo'}elseif($classes -contains 'computer'){'computer'}elseif($classes -contains 'group'){'group'}elseif($classes -contains 'user'){'account'}else{'ou'}
    $name=@(Values $r 'sAMAccountName');if(-not $name.Count){$name=@(Values $r 'name')}
    $parent=$r.DistinguishedName -replace '^(?:\\.|[^,])+,',''
    $props=@{description=(@(Values $r 'description') -join '');firstname=(@(Values $r 'givenName') -join '');lastname=(@(Values $r 'sn') -join '');displayName=(@(Values $r 'displayName') -join '');mailPrimaryAddress=(@(Values $r 'mail') -join '')}
    if($kind -eq 'account'){$props.disabled=([int](@(Values $r 'userAccountControl')[0]) -band 2) -ne 0}
    if($kind -eq 'gpo'){
      $name=@(Values $r 'displayName')
      $recipe=Join-Path $config.StateDirectory ((Guid-Of $r)+'.recipe.json')
      if(Test-Path -LiteralPath $recipe){$props.definition=Get-Content -LiteralPath $recipe -Raw|ConvertFrom-Json}
    }
    $members=@(Values $r 'member' | ForEach-Object {$byDn[$_.ToLowerInvariant()]} | Where-Object {$_})
    if($kind -eq 'gpo'){
      $needle='CN={'+(Guid-Of $r)+'},'
      $members=@($rows|Where-Object {(@(Values $_ 'gPLink') -join '').IndexOf($needle,[StringComparison]::OrdinalIgnoreCase) -ge 0}|ForEach-Object {Guid-Of $_})
    }
    $version=if($kind -eq 'gpo'){Gpo-Version (Guid-Of $r)}else{@(Values $r 'uSNChanged') -join ''}
    $objects+=@{id=(Guid-Of $r);dn=$r.DistinguishedName;name=($name -join '');kind=$kind;parentId=$byDn[$parent.ToLowerInvariant()];version=$version;module='ad';properties=$props;memberIds=$members}
  }
  return @{objects=$objects;fields=@('firstname','lastname','displayName','description','mailPrimaryAddress')|ForEach-Object {@{name=$_;type='text';read=$true;write=$true}};providerVersion=('Windows RSAT '+(Get-Module GroupPolicy).Version);observedAt=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();complete=$true}
}
function Equal-Bytes([byte[]]$a,[byte[]]$b) {
  if($a.Length -ne $b.Length){return $false};$diff=0
  for($i=0;$i -lt $a.Length;$i++){$diff=$diff -bor ($a[$i] -bxor $b[$i])}
  return $diff -eq 0
}
function Save-State([string]$path,$state) {
  $tmp=$path+'.tmp';$state|ConvertTo-Json -Depth 40|Set-Content -LiteralPath $tmp -Encoding UTF8
  Move-Item -LiteralPath $tmp -Destination $path -Force
}
function Assert-Version($object) {
  $actual=Read-Object $object.dn
  $expected=if($object.module -eq 'ad'){$object}else{$object.properties.windowsIdentity}
  if(-not $expected){throw 'WINDOWS_IDENTITY_NOT_OBSERVED'}
  $version=if($object.kind -eq 'gpo'){Gpo-Version (Guid-Of $actual)}else{@(Values $actual 'uSNChanged') -join ''}
  if((Guid-Of $actual) -ne $expected.id -or $version -ne $expected.version){throw 'DIRECTORY_VERSION_CHANGED'}
  return $actual
}
function Assertion-Control([string]$version){
  # RFC 4528 equalityMatch: uSNChanged=<reviewed version>, BER-encoded.
  $a=[Text.Encoding]::UTF8.GetBytes('uSNChanged');$b=[Text.Encoding]::UTF8.GetBytes($version)
  $body=[byte[]](@(4,$a.Length)+$a+@(4,$b.Length)+$b)
  $encoded=[byte[]](@(163,$body.Length)+$body)
  return New-Object System.DirectoryServices.Protocols.DirectoryControl('1.3.6.1.1.12',$encoded,$true,$true)
}
function Modify-Ldap($target,$changes) {
  $request=New-Object System.DirectoryServices.Protocols.ModifyRequest($target.dn)
  foreach($key in $changes.Keys){
    $m=New-Object System.DirectoryServices.Protocols.DirectoryAttributeModification
    $m.Name=$key;$m.Operation=[System.DirectoryServices.Protocols.DirectoryAttributeOperation]::Replace
    foreach($value in @($changes[$key])){if($null -ne $value){$m.Add([string]$value)|Out-Null}}
    $request.Modifications.Add($m)|Out-Null
  }
  $request.Controls.Add((Assertion-Control $target.version))|Out-Null
  $ldap.SendRequest($request)|Out-Null
}
function Apply-Operation($body) {
  $id=[Guid]::Parse($body.operationId).ToString()
  $path=Join-Path $config.StateDirectory ($id+'.json')
  $serialized=$body|ConvertTo-Json -Depth 40 -Compress
  $digest=([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($serialized)))).Replace('-','')
  $state=if(Test-Path -LiteralPath $path){Get-Content -LiteralPath $path -Raw|ConvertFrom-Json}else{[pscustomobject]@{digest=$digest;status='new';gpoId=$null;result=$null;step=0;gpoVersion=$null}}
  if($state.digest -ne $digest){throw 'IDEMPOTENCY_CONFLICT'}
  if($state.status -eq 'completed'){return $state.result}
  $action=$body.action
  function Target([string]$id){$item=@($body.before.objects|Where-Object {$_.id -eq $id});if($item.Count -ne 1){throw 'DIRECTORY_TARGET_MISSING'};return $item[0]}
  if($action.kind -like 'gpo.*'){
    $definition=if($action.kind -eq 'gpo.publish'){$action.definition}else{$null}
    $ou=Target $(if($definition){$definition.targetOuId}else{$action.parentId})
    $filter=Target $(if($definition){$definition.filterGroupId}else{$action.filterGroupId})
    if($ou.kind -ne 'ou' -or $filter.kind -ne 'group'){throw 'GPO_TARGET_INVALID'}
    Assert-Boundary $ou.dn
    Assert-Version $ou|Out-Null;Assert-Version $filter|Out-Null
    if($definition -and $definition.scope -eq 'computer'){
      $computerTarget=Target $definition.computerTargetId
      # The explicit target must be the linked OU or the applied filter group.
      if($computerTarget.id -ne $ou.id -and $computerTarget.id -ne $filter.id){throw 'COMPUTER_TARGET_MUST_MATCH_SCOPE'}
    }
    if($state.gpoId){$gpo=Get-GPO -Guid $state.gpoId -Domain $config.Domain -Server $config.Server}
    elseif($action.objectId){
      $target=Target $action.objectId;Assert-Version $target|Out-Null
      $gpoGuid=[regex]::Match($target.dn,'CN=\{([0-9a-f-]+)\}',[Text.RegularExpressions.RegexOptions]::IgnoreCase).Groups[1].Value
      $gpo=Get-GPO -Guid $gpoGuid -Domain $config.Domain -Server $config.Server
    }else{
      $existing=Get-GPO -Name $definition.name -Domain $config.Domain -Server $config.Server -ErrorAction SilentlyContinue
      if($existing -and $existing.Description -ne ('ERPX operation '+$id)){throw 'GPO_NAME_EXISTS'}
      $gpo=if($existing){$existing}else{New-GPO -Name $definition.name -Comment ('ERPX operation '+$id) -Domain $config.Domain -Server $config.Server}
    }
    $state.gpoId=$gpo.Id.ToString()
    # A changed report after an interrupted step requires a new preview; never overwrite it blindly.
    $currentVersion=Gpo-Version $state.gpoId
    if($state.gpoVersion -and $state.gpoVersion -ne $currentVersion){throw 'GPO_CONCURRENT_OR_PARTIAL_CHANGE'}
    $state.gpoVersion=$currentVersion;$state.status='running';Save-State $path $state
    function Checkpoint-Gpo {$state.gpoVersion=Gpo-Version $state.gpoId;Save-State $path $state}
    function Assert-Gpo {if((Gpo-Version $state.gpoId) -ne $state.gpoVersion){throw 'GPO_CONCURRENT_OR_PARTIAL_CHANGE'}}
    # An ERPX GPO must be managed by this runner; external GPOs remain link-only until adopted explicitly.
    if($definition -and -not $gpo.Description.StartsWith('ERPX operation ')){throw 'GPO_NOT_MANAGED'}
    if($definition){
      $index=0
      foreach($setting in $body.registry){
        $index++
        if($setting.key -notmatch '^HK(LM|CU)\\Software\\(Policies\\Microsoft\\Windows\\(RemovableStorageDevices|Control Panel\\Desktop)|Microsoft\\Windows\\CurrentVersion\\Policies\\System)$' -or $setting.name -notin @('Deny_All','InactivityTimeoutSecs','ScreenSaveActive','ScreenSaverIsSecure','ScreenSaveTimeOut','SCRNSAVE.EXE','Wallpaper','WallpaperStyle')){throw 'REGISTRY_SETTING_UNSUPPORTED'}
        if($index -le $state.step){continue}
        Assert-Gpo
        Set-GPRegistryValue -Guid $gpo.Id -Domain $config.Domain -Server $config.Server -Key $setting.key -ValueName $setting.name -Type $setting.type -Value $setting.value|Out-Null
        $read=Get-GPRegistryValue -Guid $gpo.Id -Domain $config.Domain -Server $config.Server -Key $setting.key -ValueName $setting.name
        if([string]$read.Value -ne [string]$setting.value){throw 'GPO_READBACK_FAILED'}
        $state.step=$index;Checkpoint-Gpo
      }
      $dn='CN={'+$gpo.Id.ToString().ToUpperInvariant()+'},CN=Policies,CN=System,'+$config.BaseDn
      $gpoObject=Read-Object $dn
      $root=@(Values $gpoObject 'gPCFileSysPath')[0]
      if(-not $root.StartsWith('\\'+$config.Domain+'\',[StringComparison]::OrdinalIgnoreCase)-and-not $root.StartsWith('\\'+$config.Server+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'SYSVOL_PATH_INVALID'}
      $recipePath=Join-Path $config.StateDirectory ($gpo.Id.ToString()+'.recipe.json')
      # Only runner-owned files are removed; omitting a preference stops publishing that recipe.
      foreach($relative in @('User/Preferences/Drives/Drives.xml','User/Preferences/Printers/Printers.xml')){
        if(-not @($body.preferences|Where-Object {$_.path -eq $relative}).Count){
          $destination=Join-Path $root ($relative.Replace('/','\'))
          if(Test-Path -LiteralPath $destination){Assert-Gpo;Remove-Item -LiteralPath $destination;Checkpoint-Gpo}
        }
      }
      foreach($file in $body.preferences){
        Assert-Gpo
        if($file.path -notin @('User/Preferences/Drives/Drives.xml','User/Preferences/Printers/Printers.xml')){throw 'PREFERENCE_PATH_INVALID'}
        $settings=New-Object Xml.XmlReaderSettings;$settings.DtdProcessing=[Xml.DtdProcessing]::Prohibit;$settings.XmlResolver=$null
        $reader=[Xml.XmlReader]::Create((New-Object IO.StringReader($file.xml)),$settings)
        $document=New-Object Xml.XmlDocument;$document.XmlResolver=$null;$document.Load($reader);$reader.Dispose()
        if($document.SelectNodes('//*[@cpassword or @userName or @username]').Count){throw 'PREFERENCE_CREDENTIALS_FORBIDDEN'}
        $destination=Join-Path $root ($file.path.Replace('/','\'))
        New-Item -ItemType Directory -Path (Split-Path $destination) -Force|Out-Null
        $existing=if(Test-Path -LiteralPath $destination){Get-Content -LiteralPath $destination -Raw}else{''}
        if($existing -eq $document.OuterXml){continue}
        [IO.File]::WriteAllText($destination,$document.OuterXml,[Text.Encoding]::UTF8)
        $check=New-Object Xml.XmlDocument;$check.Load($destination)
        if($check.OuterXml -ne $document.OuterXml){throw 'PREFERENCE_READBACK_FAILED'}
        $entry=Read-Object $dn
        $extensions=@(Values $entry 'gPCUserExtensionNames') -join ''
        $pair='['+$file.extension+$file.toolExtension+']'
        if(-not $extensions.Contains($pair)){$extensions+=$pair}
        $version=[int](@(Values $entry 'versionNumber')[0])
        $version=(((($version -shr 16)+1) -band 65535) -shl 16) -bor ($version -band 65535)
        Modify-Ldap @{dn=$dn;version=(@(Values $entry 'uSNChanged') -join '')} @{gPCUserExtensionNames=$extensions;versionNumber=$version}
        $gpt=Join-Path $root 'GPT.INI';$text=Get-Content -LiteralPath $gpt -Raw
        [IO.File]::WriteAllText($gpt,([regex]::Replace($text,'(?im)^Version=\d+','Version='+$version)),[Text.Encoding]::ASCII)
        Checkpoint-Gpo
      }
    }
    Assert-Gpo
    $filterEntry=Read-Object $filter.dn
    # Resolve the group via its DN through LDAP, then apply the corresponding account name.
    $filterName=@(Values $filterEntry 'sAMAccountName')[0]
    foreach($permission in @(Get-GPPermission -Guid $gpo.Id -All -Domain $config.Domain -Server $config.Server)){
      if($definition -and $permission.Permission -eq 'GpoApply' -and $permission.Trustee.Name -ne $filterName){
        Set-GPPermission -Guid $gpo.Id -TargetName $permission.Trustee.Name -TargetType Group -PermissionLevel GpoRead -Replace -Domain $config.Domain -Server $config.Server|Out-Null
      }
    }
    Set-GPPermission -Guid $gpo.Id -TargetName $filterName -TargetType Group -PermissionLevel GpoApply -Domain $config.Domain -Server $config.Server|Out-Null
    Checkpoint-Gpo
    $links=@((Get-GPInheritance -Target $ou.dn -Domain $config.Domain -Server $config.Server).GpoLinks)
    if(-not ($links|Where-Object {$_.GpoId -eq $gpo.Id})){New-GPLink -Guid $gpo.Id -Target $ou.dn -LinkEnabled Yes -Domain $config.Domain -Server $config.Server|Out-Null}
    Checkpoint-Gpo
    $confirmed=@((Get-GPInheritance -Target $ou.dn -Domain $config.Domain -Server $config.Server).GpoLinks|Where-Object {$_.GpoId -eq $gpo.Id -and $_.Enabled})
    if(-not $confirmed.Count){throw 'GPO_LINK_READBACK_FAILED'}
    $permission=Get-GPPermission -Guid $gpo.Id -TargetName $filterName -TargetType Group -Domain $config.Domain -Server $config.Server
    if($permission.Permission -ne 'GpoApply'){throw 'GPO_FILTER_READBACK_FAILED'}
    if($definition){Save-State $recipePath $definition}
    $report=Get-GPOReport -Guid $gpo.Id -ReportType Xml -Domain $config.Domain -Server $config.Server
    $reportPath=Join-Path $config.StateDirectory ($id+'.gpo.xml');[IO.File]::WriteAllText($reportPath,$report,[Text.Encoding]::UTF8)
    $result=@{verified=$true;objectId=$gpo.Id.ToString();published=$true;endpointConfirmed=$false;reportHash=([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($report)))).Replace('-','')}
  }else{
    $target=if($action.PSObject.Properties.Name -contains 'objectId'){Target $action.objectId}else{$null}
    if($target){
      Assert-Boundary $target.dn;$entry=Assert-Version $target
      $changes=@{}
      if($action.kind -eq 'account.disable'){$changes.userAccountControl=[int](@(Values $entry 'userAccountControl')[0]) -bor 2}
      elseif($action.kind -eq 'account.enable'){$changes.userAccountControl=[int](@(Values $entry 'userAccountControl')[0]) -band (-bnot 2)}
      elseif($action.kind -eq 'account.update'){
        $map=@{firstname='givenName';lastname='sn';displayName='displayName';description='description';mailPrimaryAddress='mail'}
        foreach($p in $action.fields.PSObject.Properties){if(-not $map.ContainsKey($p.Name)){throw 'FIELD_UNSUPPORTED'};$changes[$map[$p.Name]]=$p.Value}
      }elseif($action.kind -eq 'group.members'){
        $members=@(Values $entry 'member')
        foreach($memberId in $action.memberIds){$member=Target $memberId;Assert-Version $member|Out-Null;if($action.action -eq 'add'){$members+=,$member.dn}else{$members=@($members|Where-Object {$_ -ne $member.dn})}}
        $changes.member=@($members|Select-Object -Unique)
      }elseif($action.kind -eq 'object.move'){
        $parent=Target $action.parentId;Assert-Boundary $parent.dn;Assert-Version $parent|Out-Null
        $rdn=[regex]::Match($target.dn,'^(?:\\.|[^,])+').Value
        $move=New-Object System.DirectoryServices.Protocols.ModifyDNRequest($target.dn,$parent.dn,$rdn)
        $move.Controls.Add((Assertion-Control $target.version))|Out-Null;$ldap.SendRequest($move)|Out-Null
        $target.dn=$rdn+','+$parent.dn
      }else{throw 'DIRECTORY_ACTION_UNSUPPORTED'}
      if($changes.Count){Modify-Ldap $target $changes}
      $after=Read-Object $target.dn
      foreach($k in $changes.Keys){if(((@(Values $after $k)|Sort-Object)-join '|') -ne ((@($changes[$k])|ForEach-Object {[string]$_}|Sort-Object)-join '|')){throw 'DIRECTORY_READBACK_FAILED'}}
      $result=@{verified=$true;objectId=(Guid-Of $after);version=(@(Values $after 'uSNChanged') -join '');dn=$target.dn}
    }elseif($action.kind -in @('account.create','group.create')){
      $parent=Target $action.parentId;Assert-Boundary $parent.dn;Assert-Version $parent|Out-Null
      $name=if($action.kind -eq 'account.create'){$action.username}else{$action.name}
      if($name -notmatch '^[a-zA-Z0-9][a-zA-Z0-9 ._-]{1,99}$'){throw 'DIRECTORY_NAME_INVALID'}
      $dn='CN='+$name+','+$parent.dn;$add=New-Object System.DirectoryServices.Protocols.AddRequest($dn)
      $props=@{objectClass=if($action.kind -eq 'account.create'){@('top','person','organizationalPerson','user')}else{@('top','group')};sAMAccountName=$name;description='ERPX operation '+$id}
      if($action.kind -eq 'account.create'){$props.givenName=$action.firstName;$props.sn=$action.lastName;$props.userAccountControl='514'}else{$props.groupType='-2147483646'}
      foreach($k in $props.Keys){$attribute=New-Object System.DirectoryServices.Protocols.DirectoryAttribute;$attribute.Name=$k;foreach($v in @($props[$k])){$attribute.Add([string]$v)|Out-Null};$add.Attributes.Add($attribute)|Out-Null}
      try{$ldap.SendRequest($add)|Out-Null}catch [System.DirectoryServices.Protocols.DirectoryOperationException]{if($_.Exception.Response.ResultCode -ne 'EntryAlreadyExists'){throw}}
      $after=Read-Object $dn;if((@(Values $after 'description') -join '') -ne ('ERPX operation '+$id)){throw 'DIRECTORY_OBJECT_EXISTS'}
      $result=@{verified=$true;objectId=(Guid-Of $after);version=(@(Values $after 'uSNChanged') -join '');dn=$dn}
    }else{throw 'DIRECTORY_ACTION_UNSUPPORTED'}
  }
  $state.status='completed';$state.result=$result;Save-State $path $state;return $result
}
$listener=New-Object Net.HttpListener
$listener.Prefixes.Add($config.Prefix);$listener.Start()
try {
  while($listener.IsListening){
    $context=$listener.GetContext();$response=$context.Response
    try{
      $provided=$context.Request.Headers['Authorization']
      if(-not (Equal-Bytes ([Text.Encoding]::UTF8.GetBytes([string]$provided)) ([Text.Encoding]::UTF8.GetBytes('Bearer '+$token)))){throw 'UNAUTHORIZED'}
      if($context.Request.HttpMethod -ne 'POST' -or $context.Request.ContentLength64 -lt 0 -or $context.Request.ContentLength64 -gt 12000000){throw 'REQUEST_INVALID'}
      $reader=New-Object IO.StreamReader($context.Request.InputStream)
      $body=$reader.ReadToEnd()|ConvertFrom-Json;$reader.Dispose()
      if($body.connectionId -ne $config.ConnectionId){throw 'CONNECTION_FORBIDDEN'}
      $route=$context.Request.Url.AbsolutePath.TrimEnd('/').Split('/')[-1]
      $result=if($route -eq 'inventory'){Inventory}elseif($route -eq 'apply'){Apply-Operation $body}else{throw 'ROUTE_NOT_FOUND'}
      $output=$result|ConvertTo-Json -Depth 40 -Compress;$response.StatusCode=200
    }catch{
      # Provider exception details can contain directory data; return only a controlled code.
      $code=if($_.Exception.Message -match '^[A-Z_]+$'){$_.Exception.Message}else{'DIRECTORY_OPERATION_FAILED'}
      $response.StatusCode=if($code -eq 'UNAUTHORIZED'){401}else{409}
      $output=@{error=$code}|ConvertTo-Json -Compress
    }
    $bytes=[Text.Encoding]::UTF8.GetBytes($output);$response.ContentType='application/json';$response.ContentLength64=$bytes.Length
    $response.OutputStream.Write($bytes,0,$bytes.Length);$response.Close()
  }
}finally{$listener.Stop();$listener.Close();$ldap.Dispose()}
