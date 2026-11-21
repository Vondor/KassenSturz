<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# KassenSturz installieren

*English: [INSTALL.md](INSTALL.md).* Die stabile Ausgabe ist für den Alltag mit Ihrem eigenen Haushaltsbuch gedacht; legen Sie von Zeit zu Zeit eine eigene Kopie an. Beta-Ausgaben sind zum Ausprobieren da: Nehmen Sie dort ein Beispielbuch oder eine Kopie Ihres Buchs (D-954). Schritt-für-Schritt-Blätter für Testsitzungen liegen in [`testing/`](testing/README.md).

## Linux

Es gibt zwei Wege. Beide installieren dieselbe App; nehmen Sie einen, nicht beide.

### A. Aus dem Snap Store (empfohlen)

```sh
sudo snap install kassensturz
```

Das installiert die stabile Ausgabe. Für die neueste Beta-Version, die mehr Fehler haben kann, nehmen Sie stattdessen `sudo snap install kassensturz --beta` (oder `sudo snap refresh kassensturz --beta` zum Wechseln). Starten Sie danach **KassenSturz** aus dem Anwendungsmenü. Aktualisierungen kommen von selbst, wie bei jedem Snap.

**Ihr Buch liegt in `Dokumente/KassenSturz`, darum bleibt es beim Entfernen des Snaps erhalten (D-934, B-429).** Ab dieser Version liegen Buch, Sicherungen und die Erweiterungen eines Snaps in einem Ordner `KassenSturz` in Ihrem Dokumente-Ordner (dafür braucht der Snap seinen Zugriff `home`, den Desktops von selbst verbinden); nach einer Neuinstallation findet die App das Buch wieder. Die Einstellungen der Bankverbindung bleiben im eigenen Ordner des Snaps (sie kommen nie in einen Ordner, der synchronisiert werden könnte); nach dem Entfernen des Snaps richten Sie die Bankverbindung neu ein. **Ein Buch, das ein früherer Snap angelegt hat, liegt im eigenen Ordner des Snaps** `~/snap/kassensturz/`, den `sudo snap remove kassensturz` löscht: Beim Start bietet die App an, dieses Buch nach `Dokumente/KassenSturz` zu verschieben (vorher mit einer Sicherung); nehmen Sie das an, oder kopieren Sie das Buch selbst woanders hin, bevor Sie den Snap entfernen. snapd hebt 31 Tage lang eine Kopie auf (`snap saved` zeigt sie, `sudo snap restore <Nummer>` holt sie zurück, nachdem Sie den Snap wieder installiert haben), aber verlassen Sie sich nicht darauf: Kopieren Sie die Bücher, die Sie behalten möchten, vorher an einen anderen Ort, oder nutzen Sie in der App „Das ganze Haushaltsbuch speichern“. Ein Buch in einem eigenen Ordner, etwa in Dokumente, bleibt unberührt.

Stabile Ausgaben liegen im Kanal *stable* des Stores, Beta-Versionen im Kanal *beta* (D-948). Einiges ist ausgeschaltet, bis Sie es erlauben, weil es andere Teile Ihres Computers erreicht:

- die Banking-PIN im Passwortspeicher sichern: `sudo snap connect kassensturz:password-manager-service` (ohne das fragt die App jedes Mal nach der PIN);
- ein Chipkartenleser für die Bankanmeldung: `sudo snap connect kassensturz:pcscd`;
- Bücher auf USB-Laufwerken: `sudo snap connect kassensturz:removable-media`.

Diese Schalter sehen und ändern Sie mit `snap connections kassensturz` oder in KDE Discover und in Ubuntus Einstellungen bei den Berechtigungen der App. Einzelheiten (Englisch): [`SNAP.md`](SNAP.md).

### B. Die Flatpak-Datei (direkter Download)

Auf der [Release-Seite](https://github.com/Vondor/KassenSturz/releases) hat die Ausgabe mit der Markierung *Latest* (stabil; Einträge mit *Pre-release* sind Beta-Versionen) unter **Assets** eine Datei, die auf `.flatpak` endet. Fügen Sie einmal pro Computer den Katalog mit der KDE-Laufzeitumgebung hinzu und installieren Sie dann die Datei:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.kde.Platform//6.11
flatpak install --user KassenSturz-<version>-x86_64.flatpak
```

KassenSturz selbst ist nicht auf Flathub (D-765); aus dem Katalog kommt nur die KDE-Laufzeitumgebung. Eine neuere Datei installieren Sie genauso über die alte. Entfernen: `flatpak uninstall --user io.github.vondor.KassenSturz`.

Beim Entfernen des Flatpaks bleibt sein Ordner `~/.var/app/io.github.vondor.KassenSturz/` mit den Büchern und Sicherungen erhalten, außer Sie geben `--delete-data` an.

## Windows

### A. Aus dem Microsoft Store (empfohlen)

Installieren Sie KassenSturz aus dem [Microsoft Store](https://apps.microsoft.com/detail/9NM5V48PPRLQ). Der Store signiert das Programm und installiert Aktualisierungen selbst, deshalb zeigt Windows keine Warnung.

### B. Das Installationsprogramm von der Release-Seite

Laden Sie auf der [Release-Seite](https://github.com/Vondor/KassenSturz/releases) das Installationsprogramm der neuesten Ausgabe herunter und starten Sie es. Es ist nicht signiert, deshalb zeigt Windows zuerst eine SmartScreen-Warnung (D-542, [`CODE_SIGNING_POLICY.md`](CODE_SIGNING_POLICY.md)). Einzelheiten zu den Windows-Paketen (Englisch): [`WINDOWS.md`](WINDOWS.md).

## macOS

Eine signierte Version gibt es noch nicht; sie folgt, sobald ihre Signatur finanziert ist (D-048). Die Ausgaben enthalten eine **experimentelle, ungetestete** `.dmg` für Macs mit Apple-Chip, die macOS zuerst blockiert, weil sie nicht notarisiert ist. So öffnen Sie sie trotzdem: [`MACOS.de.md`](MACOS.de.md) (D-875).
