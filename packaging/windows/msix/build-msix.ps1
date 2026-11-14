# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Packs the unpacked portable zip into an unsigned MSIX for the Microsoft
# Store (#927). The Store signs the package itself, so CI builds it unsigned.
#
#   build-msix.ps1 -Root <folder with bin\kassensturz.exe> -Version 0.261114.3 -Out <file.msix>
#
# An MSIX version has four parts of at most 65535 each, so the build number
# MAJOR.YYMMDD.N is mapped to (MAJOR+1).YYMM.(DD*1000+N).0, which keeps the
# order of builds and fits (N below 1000). docs/WINDOWS.md has the rule.
param(
    [Parameter(Mandatory)][string]$Root,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Out
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Resolve-Path "$here\..\..\.."

if ($Version -notmatch '^(\d+)\.(\d{2})(\d{2})(\d{2})\.(\d+)$') { throw "Unexpected build number: $Version" }
$major = [int]$Matches[1]; $yymm = [int]($Matches[2] + $Matches[3]); $dd = [int]$Matches[4]; $n = [int]$Matches[5]
if ($n -ge 1000) { throw "More than 999 builds in one day do not fit an MSIX version" }
$msixVersion = "{0}.{1}.{2}.0" -f ($major + 1), $yymm, ($dd * 1000 + $n)
Write-Output "MSIX version $msixVersion (build $Version)"

# The layout: the program folder as in the zip, plus the manifest and logos.
$stage = Join-Path $env:RUNNER_TEMP "ks-msix"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory $stage | Out-Null
$exe = Get-ChildItem $Root -Recurse -Filter kassensturz.exe | Where-Object { $_.Directory.Name -eq "bin" } | Select-Object -First 1
if (-not $exe) { throw "bin\kassensturz.exe not found under $Root" }
Copy-Item -Recurse "$($exe.Directory.Parent.FullName)\*" $stage

$assets = New-Item -ItemType Directory "$stage\Assets"
$source = [System.Drawing.Image]::FromFile((Resolve-Path "$repo\data\icons\hicolor\256x256\apps\io.github.vondor.KassenSturz.png").Path)
try {
    foreach ($logo in @{ "StoreLogo.png" = 50; "Square44x44Logo.png" = 44; "Square150x150Logo.png" = 150 }.GetEnumerator()) {
        $size = $logo.Value
        $bitmap = New-Object System.Drawing.Bitmap $size, $size
        $g = [System.Drawing.Graphics]::FromImage($bitmap)
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.DrawImage($source, 0, 0, $size, $size)
        $g.Dispose()
        $bitmap.Save("$assets\$($logo.Key)", [System.Drawing.Imaging.ImageFormat]::Png)
        $bitmap.Dispose()
    }
} finally { $source.Dispose() }

$manifest = (Get-Content -Raw "$here\AppxManifest.xml").Replace("@VERSION@", $msixVersion)
[System.IO.File]::WriteAllText("$stage\AppxManifest.xml", $manifest, (New-Object System.Text.UTF8Encoding $false))

$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $makeappx) { throw "makeappx.exe (Windows SDK) not found" }
if (Test-Path $Out) { Remove-Item -Force $Out }
& $makeappx.FullName pack /d $stage /p $Out /o
if ($LASTEXITCODE -ne 0) { throw "makeappx pack failed" }

# Read it back: the identity the Store expects, and the version.
$check = Join-Path $env:RUNNER_TEMP "ks-msix-check"
if (Test-Path $check) { Remove-Item -Recurse -Force $check }
& $makeappx.FullName unpack /p $Out /d $check /o
if ($LASTEXITCODE -ne 0) { throw "makeappx unpack failed" }
[xml]$xml = Get-Content -Raw "$check\AppxManifest.xml"
$id = $xml.Package.Identity
if ($id.Name -ne "47425SerpentSoftware.KassenSturz-TheHouseholdBudge") { throw "Unexpected package name $($id.Name)" }
if ($id.Publisher -ne "CN=734D2F8D-959F-4381-AD8C-FF9F9E9C279C") { throw "Unexpected publisher $($id.Publisher)" }
if ($id.Version -ne $msixVersion) { throw "Unexpected version $($id.Version)" }
if (-not (Test-Path "$check\bin\kassensturz.exe")) { throw "bin\kassensturz.exe is missing in the package" }
Write-Output "MSIX ok: $Out ($((Get-Item $Out).Length) bytes)"
