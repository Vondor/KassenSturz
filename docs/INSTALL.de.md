<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# KassenSturz installieren

*English: [INSTALL.md](INSTALL.md).* KassenSturz ist in der Beta. Probieren Sie es mit erfundenen Zahlen oder einem Beispielbuch aus, nie mit Ihren eigenen Daten. Schritt-für-Schritt-Blätter für Testsitzungen liegen in [`testing/`](testing/README.md).

## Linux

Es gibt zwei Wege. Beide installieren dieselbe App; nehmen Sie einen, nicht beide.

### A. Aus dem Snap Store (empfohlen)

```sh
sudo snap install kassensturz --beta
```

Starten Sie danach **KassenSturz** aus dem Anwendungsmenü. Aktualisierungen kommen von selbst, wie bei jedem Snap.

**Bevor Sie den Snap entfernen, holen Sie Ihre Bücher heraus.** `sudo snap remove kassensturz` löscht den eigenen Ordner des Snaps, `~/snap/kassensturz/`, und ein Buch vom ersten Start liegt dort, mit seinen Sicherungen. snapd hebt 31 Tage lang eine Kopie auf (`snap saved` zeigt sie, `sudo snap restore <Nummer>` holt sie zurück, nachdem Sie den Snap wieder installiert haben), aber verlassen Sie sich nicht darauf: Kopieren Sie die Bücher, die Sie behalten möchten, vorher an einen anderen Ort, oder nutzen Sie in der App „Das ganze Haushaltsbuch speichern“. Ein Buch in einem eigenen Ordner, etwa in Dokumente, bleibt unberührt.

Die Beta liegt im Kanal *beta* des Stores; vor 1.0 gibt es keine Ausgabe im Kanal *stable*. Einiges ist ausgeschaltet, bis Sie es erlauben, weil es andere Teile Ihres Computers erreicht:

- die Banking-PIN im Passwortspeicher sichern: `sudo snap connect kassensturz:password-manager-service` (ohne das fragt die App jedes Mal nach der PIN);
- ein Chipkartenleser für die Bankanmeldung: `sudo snap connect kassensturz:pcscd`;
- Bücher auf USB-Laufwerken: `sudo snap connect kassensturz:removable-media`.

Diese Schalter sehen und ändern Sie mit `snap connections kassensturz` oder in KDE Discover und in Ubuntus Einstellungen bei den Berechtigungen der App. Einzelheiten (Englisch): [`SNAP.md`](SNAP.md).

### B. Die Flatpak-Datei (direkter Download)

Auf der [Release-Seite](https://github.com/Vondor/KassenSturz/releases) hat die neueste Ausgabe unter **Assets** eine Datei, die auf `.flatpak` endet. Fügen Sie einmal pro Computer den Katalog mit der KDE-Laufzeitumgebung hinzu und installieren Sie dann die Datei:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.kde.Platform//6.11
flatpak install --user KassenSturz-<version>-x86_64.flatpak
```

KassenSturz selbst ist nicht auf Flathub (D-765); aus dem Katalog kommt nur die KDE-Laufzeitumgebung. Eine neuere Datei installieren Sie genauso über die alte. Entfernen: `flatpak uninstall --user io.github.vondor.KassenSturz`.

Beim Entfernen des Flatpaks bleibt sein Ordner `~/.var/app/io.github.vondor.KassenSturz/` mit den Büchern und Sicherungen erhalten, außer Sie geben `--delete-data` an.

## Windows

Laden Sie auf der [Release-Seite](https://github.com/Vondor/KassenSturz/releases) das Installationsprogramm der neuesten Ausgabe herunter und starten Sie es. Es ist noch nicht signiert, deshalb zeigt Windows zuerst eine SmartScreen-Warnung (D-542, [`CODE_SIGNING_POLICY.md`](CODE_SIGNING_POLICY.md)). Das Paket für den Microsoft Store ist unterwegs; Einzelheiten (Englisch) in [`WINDOWS.md`](WINDOWS.md).

## macOS

Noch nicht. Ein signiertes `.dmg` folgt, sobald seine Signatur finanziert ist (D-048).
