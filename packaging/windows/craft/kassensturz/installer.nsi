; SPDX-FileCopyrightText: 2026 KassenSturz contributors
; SPDX-License-Identifier: GPL-3.0-or-later

; KassenSturz's installer for Windows (M7a.7, D-583). KDE Craft fills in the
; @{...} values when it packs the installer (craft --package with
; NullsoftInstallerPackager; kassensturz.py points Craft at this file).
;
; It installs for the person who runs it, into their own
; %LOCALAPPDATA%\Programs\KassenSturz, so Windows never asks for an
; administrator; it speaks German on a German Windows and English otherwise.
; Craft's packager hands it the program as one 7z archive plus 7-Zip's
; 7za.exe, which unpacks it and is then deleted (docs/THIRD_PARTY.md).

Unicode true
ManifestDPIAware true
XPStyle on

!define uninstkey "Software\Microsoft\Windows\CurrentVersion\Uninstall\KassenSturz"
!define appkey "Software\KassenSturz"
!define program "bin\kassensturz.exe"

Name "@{productname}"
Caption "@{productname} @{version}"
BrandingText "@{productname} @{version}"
OutFile "@{setupname}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\KassenSturz"
InstallDirRegKey HKCU "${appkey}" "Install_Dir"
CRCCheck on
SetDateSave on

!include "MUI2.nsh"
!include "LogicLib.nsh"

!define MUI_ICON "@{icon}"
!define MUI_UNICON "@{icon}"
!define MUI_ABORTWARNING

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; The first language is the one used when Windows speaks neither.
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "German"

LangString stillRunning ${LANG_ENGLISH} "KassenSturz is still open. Close it, then click Retry. Your books stay as they are."
LangString stillRunning ${LANG_GERMAN} "KassenSturz ist noch geöffnet. Schließen Sie es und klicken Sie dann auf Wiederholen. Ihre Haushaltsbücher bleiben, wie sie sind."
LangString unpackFailed ${LANG_ENGLISH} "KassenSturz's files could not be unpacked. Check that the drive has space (about 600 MB), then run this installer again."
LangString unpackFailed ${LANG_GERMAN} "Die Dateien von KassenSturz konnten nicht entpackt werden. Prüfen Sie, ob auf dem Laufwerk Platz ist (etwa 600 MB), und starten Sie diese Installation noch einmal."

; Files of a running program can't be replaced, so an update waits until
; KassenSturz is closed. A running program can't be opened for writing, which
; is how the installer tells. The person closes it: the installer never ends
; it, so nothing that is being typed is lost.
!macro WAIT_UNTIL_CLOSED prefix
    Function ${prefix}WaitUntilClosed
        retry:
        ${If} ${FileExists} "$INSTDIR\${program}"
            ClearErrors
            FileOpen $0 "$INSTDIR\${program}" a
            ${If} ${Errors}
                MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "$(stillRunning)" IDRETRY retry
                Abort
            ${EndIf}
            FileClose $0
        ${EndIf}
    FunctionEnd
!macroend
!insertmacro WAIT_UNTIL_CLOSED ""
!insertmacro WAIT_UNTIL_CLOSED "un."

Section
    Call WaitUntilClosed
    ; An earlier version's own uninstaller removes exactly its own files first,
    ; so no file of it is left beside the new ones. It touches no book.
    ${If} ${FileExists} "$INSTDIR\uninstall.exe"
        ExecWait '"$INSTDIR\uninstall.exe" /S _?=$INSTDIR'
    ${EndIf}

    SetOutPath "$INSTDIR"
    File /a "@{dataPath}"
    File /a "@{7za}"
    File /a "@{icon}"
    nsExec::ExecToLog '"$INSTDIR\7za.exe" x -r -y "$INSTDIR\@{dataName}" -o"$INSTDIR"'
    Pop $0
    Delete "$INSTDIR\7za.exe"
    Delete "$INSTDIR\@{dataName}"
    ${If} $0 != 0
        MessageBox MB_OK|MB_ICONSTOP "$(unpackFailed)"
        Abort
    ${EndIf}
    AddSize @{installSize}

    WriteUninstaller "$INSTDIR\uninstall.exe"
    WriteRegStr HKCU "${appkey}" "Install_Dir" "$INSTDIR"
    WriteRegStr HKCU "${uninstkey}" "DisplayName" "@{productname}"
    WriteRegStr HKCU "${uninstkey}" "DisplayVersion" "@{version}"
    WriteRegStr HKCU "${uninstkey}" "Publisher" "@{company}"
    WriteRegStr HKCU "${uninstkey}" "URLInfoAbout" "@{website}"
    WriteRegStr HKCU "${uninstkey}" "DisplayIcon" "$INSTDIR\${program}"
    WriteRegStr HKCU "${uninstkey}" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "${uninstkey}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
    WriteRegDWORD HKCU "${uninstkey}" "EstimatedSize" @{estimated_size}
    WriteRegDWORD HKCU "${uninstkey}" "NoModify" 1
    WriteRegDWORD HKCU "${uninstkey}" "NoRepair" 1

    ; The Start menu entry starts in the person's own folder, where the file
    ; dialogs then open (M6-B147).
    SetShellVarContext current
    SetOutPath "$PROFILE"
    CreateShortCut "$SMPROGRAMS\@{productname}.lnk" "$INSTDIR\${program}"
SectionEnd

; Removes the program's files, one by one as Craft lists them, never a whole
; folder, so books and anything else the person put there stay.
Section "Uninstall"
    Call un.WaitUntilClosed
    SetShellVarContext current
    Delete "$SMPROGRAMS\@{productname}.lnk"
    @{uninstallFiles}
    @{uninstallDirs}
    RMDir "$INSTDIR"
    DeleteRegKey HKCU "${uninstkey}"
    DeleteRegKey HKCU "${appkey}"
SectionEnd
