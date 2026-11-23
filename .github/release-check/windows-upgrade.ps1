# Installs a shipped SecretFolder (the setup .exe or the per-machine MSI), puts a test vault where
# it keeps its vault, then runs the new installer over it the way the old updater does, while the
# old program is running. Checks that the vault (and ui.json beside it) is byte for byte, that
# the old program, uninstaller, entries and shortcuts are gone, that the new app is installed
# once, and that it starts in the vault's saved language.
#
# Run by release-check.yml on a fresh GitHub machine. Refuses to run where a vault exists.
param(
    [Parameter(Mandatory)] [string] $OldInstaller,
    [Parameter(Mandatory)] [string] $NewSetup,
    [Parameter(Mandatory)] [string] $Fixture,
    # A name the new app's first screen shows (the gate title in the saved language).
    [Parameter(Mandatory)] [string] $ExpectName
)
$ErrorActionPreference = 'Stop'
$vault = Join-Path $env:APPDATA 'com.ahmi.secretfolder'
$userDir = Join-Path $env:LOCALAPPDATA 'SecretFolder'
$newDir = Join-Path $env:LOCALAPPDATA 'Programs\SecretFolder'
$oldKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\SecretFolder'
$manuKey = 'HKCU:\Software\ahmi\SecretFolder'
$msi = $OldInstaller.ToLower().EndsWith('.msi')
$fail = 0
function Check($ok, $what) {
    if ($ok) { Write-Output "PASS  $what" } else { Write-Output "FAIL  $what"; $script:fail++ }
}
function Hashes($dir) {
    Get-ChildItem -LiteralPath $dir -Recurse -File | Sort-Object FullName | ForEach-Object {
        '{0}  {1}' -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash, $_.FullName.Substring($dir.Length)
    }
}
function MsiEntries() {
    foreach ($root in 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall') {
        Get-ChildItem $root -ErrorAction SilentlyContinue | ForEach-Object { Get-ItemProperty $_.PSPath } |
            Where-Object { $_.DisplayName -eq 'SecretFolder' -and $_.WindowsInstaller -eq 1 }
    }
}

if (Test-Path -LiteralPath $vault) { throw "A vault already exists at $vault. Not touching it." }

Write-Output "== 1. install $(Split-Path -Leaf $OldInstaller)"
if ($msi) {
    $p = Start-Process -FilePath 'msiexec.exe' -ArgumentList '/i', "`"$OldInstaller`"", '/quiet', '/norestart' -Wait -PassThru
    Check ($p.ExitCode -eq 0) "MSI installed (exit $($p.ExitCode))"
    $entry = @(MsiEntries)[0]
    Check ($null -ne $entry) 'MSI uninstall entry present'
    $oldExe = Get-ChildItem -Path "$env:ProgramFiles\SecretFolder", "${env:ProgramFiles(x86)}\SecretFolder" -Filter 'secretfolder.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
} else {
    Start-Process -FilePath $OldInstaller -ArgumentList '/S' -Wait
    Check (Test-Path -LiteralPath $oldKey) 'setup uninstall entry present'
    $oldExe = Get-Item -LiteralPath (Join-Path $userDir 'secretfolder.exe') -ErrorAction SilentlyContinue
}
Check ($null -ne $oldExe) "old program installed ($($oldExe.FullName))"
if ($null -eq $oldExe) { exit 1 }

Write-Output '== 2. test vault'
Copy-Item -LiteralPath $Fixture -Destination $vault -Recurse
$before = Hashes $vault
Write-Output ($before -join "`n")

Write-Output '== 3. update while the old program runs'
$old = Start-Process -FilePath $oldExe.FullName -PassThru
Start-Sleep -Seconds 6
Check (-not $old.HasExited) 'old program running'
$atStart = Hashes $vault
Check (($before -join "`n") -eq ($atStart -join "`n")) 'old program opening the vault changed nothing'
$before = $atStart
# The Tauri updater (installMode passive) starts the downloaded setup with these NSIS flags and
# quits; the new installer must take them without complaint. Silent here so nothing waits on a click.
$setup = Start-Process -FilePath $NewSetup -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/P', '/UPDATE', '/R', '/ARGS' -PassThru
Start-Sleep -Seconds 3
Check (-not $setup.HasExited) 'new setup waits while the old program runs'
Stop-Process -Id $old.Id -Force
$setup.WaitForExit(300000) | Out-Null
Check ($setup.HasExited -and $setup.ExitCode -eq 0) "new setup finished (exit $($setup.ExitCode))"
Get-Process -Name 'SecretFolder' -ErrorAction SilentlyContinue | Stop-Process -Force

Write-Output '== 4. after the update'
$after = Hashes $vault
Check (($before -join "`n") -eq ($after -join "`n")) 'vault folder byte for byte (every file, same hashes)'
Check (-not (Test-Path -LiteralPath $oldExe.FullName)) 'old program removed'
if ($msi) {
    Check (@(MsiEntries).Count -eq 0) 'MSI install removed'
} else {
    Check (-not (Test-Path -LiteralPath (Join-Path $userDir 'uninstall.exe'))) 'old uninstaller removed'
    Check (-not (Test-Path -LiteralPath $userDir)) 'old install folder gone (it held nothing else)'
    Check (-not (Test-Path -LiteralPath $oldKey)) 'old uninstall entry removed'
    Check (-not (Test-Path -LiteralPath $manuKey)) "old $manuKey removed"
}
$newExe = Join-Path $newDir 'SecretFolder.exe'
Check (Test-Path -LiteralPath $newExe) 'new app installed to %LOCALAPPDATA%\Programs\SecretFolder'
$entries = @(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' | ForEach-Object { Get-ItemProperty $_.PSPath } | Where-Object DisplayName -eq 'SecretFolder') + @(MsiEntries)
Check ($entries.Count -eq 1) "exactly one SecretFolder in Apps & features (found $($entries.Count))"
$startOld = Join-Path ([Environment]::GetFolderPath('Programs')) 'SecretFolder.lnk'
if (Test-Path -LiteralPath $startOld) {
    $target = (New-Object -ComObject WScript.Shell).CreateShortcut($startOld).TargetPath
    Check ($target -like "$newDir*") "Start menu shortcut points at the new app ($target)"
} else { Check $true 'no stale Start menu shortcut' }

Write-Output '== 5. the new app starts on the upgraded vault'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$app = Start-Process -FilePath $newExe -PassThru
$found = $false
$deadline = (Get-Date).AddSeconds(60)
$A = [System.Windows.Automation.AutomationElement]
while (-not $found -and (Get-Date) -lt $deadline -and -not $app.HasExited) {
    Start-Sleep -Milliseconds 750
    $byPid = New-Object System.Windows.Automation.PropertyCondition($A::ProcessIdProperty, $app.Id)
    foreach ($w in $A::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $byPid)) {
        $byName = New-Object System.Windows.Automation.PropertyCondition($A::NameProperty, $ExpectName)
        if ($null -ne $w.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $byName)) { $found = $true }
    }
}
Check (-not $app.HasExited) 'new app running'
Check $found "new app shows `"$ExpectName`" (its saved language)"
Start-Sleep -Seconds 3
Get-Process -Name 'SecretFolder' -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
$final = Hashes $vault
Check (($before -join "`n") -eq ($final -join "`n")) 'vault still byte for byte after the new app opened it'

Write-Output "== result: $fail failed"
exit $fail
