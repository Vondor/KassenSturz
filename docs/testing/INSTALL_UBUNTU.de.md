# KassenSturz installieren: Ubuntu

*Dieses Blatt führt Sie Schritt für Schritt durch die Installation. Sie brauchen dafür keine Hilfe. Ihr Computer-Passwort wird einmal gebraucht. Dauer: etwa 15 Minuten. Englische Fassung: [INSTALL_UBUNTU.en.md](INSTALL_UBUNTU.en.md).*

> **Stand für das Team.** Der Ablauf unten ist mit den Befehlen aus der [README](../../README.md) (Abschnitt „Trying a build from CI“) abgeglichen. Dateiname der Testversion und Bildschirmfotos von einem frischen Ubuntu werden nachgetragen, sobald es die erste Testversion gibt (M7a.4); die Stellen sind mit „Bildschirmfoto folgt“ gekennzeichnet (Liste in [README.md](README.md)).

**Kommen Sie nicht weiter?** Sagen Sie es der Testleitung. Sie installiert nicht für Sie, aber sie notiert die Stelle, und Sie bekommen einen Hinweis, damit der Test weitergeht.

---

## Was Sie brauchen

- Ubuntu (aktuelle Version mit KDE oder GNOME).
- Eine Internetverbindung für die Einrichtung.
- Etwa 3 GB freien Platz (das KDE-Laufzeitpaket ist groß).
- Ihr Computer-Passwort, einmal.

**Wie Sie Befehle eingeben:** Drücken Sie **Strg + Alt + T**. Es öffnet sich ein Fenster mit Text, das **Terminal**. Kopieren Sie jede Zeile unten dorthin (Einfügen mit **Strg + Umschalt + V**) und drücken Sie die Eingabetaste. Warten Sie, bis wieder eine Eingabezeile erscheint, bevor Sie den nächsten Befehl eingeben.

## Schritt 1: Die Download-Seite öffnen

1. Öffnen Sie den Browser und geben Sie in die Adresszeile ein:

   **https://github.com/Vondor/KassenSturz/releases**

2. Die oberste Karte ist die neueste Version. Oben steht **Pre-release** (Testversion). Das ist richtig so. Ein GitHub-Konto brauchen Sie nicht.
3. Scrollen Sie zu **Assets** (Dateien) und laden Sie die Datei herunter, die auf **.flatpak** endet; die Testleitung hat Ihnen den genauen Namen vorher gesagt. **Nicht** *Source code (zip)* und nicht die Windows-Datei.
4. Die Datei liegt danach im Ordner **Downloads**.

*(Bildschirmfoto folgt: Download-Seite mit der richtigen Datei.)*

## Schritt 2: Flatpak einrichten (einmal, braucht Ihr Passwort)

Flatpak ist die Art, wie KassenSturz unter Ubuntu installiert wird. Ubuntu bringt es nicht immer mit.

1. Im Terminal:

   ```sh
   sudo apt install flatpak
   ```

2. Es fragt nach **Ihrem Passwort**. Beim Tippen erscheint nichts auf dem Bildschirm, das ist normal. Passwort eintippen, Eingabetaste.
3. Fragt es „Möchten Sie fortfahren? [J/n]“, tippen Sie **J** und die Eingabetaste.

*(Bildschirmfoto folgt: Terminal mit der Passwortabfrage.)*

## Schritt 3: Den Software-Katalog und das KDE-Laufzeitpaket hinzufügen (ohne Passwort, nur für Sie)

KassenSturz braucht ein Laufzeitpaket von KDE. Beide Befehle brauchen **kein** Passwort und ändern nur Ihr eigenes Benutzerkonto:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.kde.Platform//6.11
```

Beim zweiten Befehl fragt Flatpak ein- oder zweimal nach einer Bestätigung. Tippen Sie jeweils **J** und die Eingabetaste. Der Download dauert einige Minuten. Warten Sie, bis wieder eine Eingabezeile da ist.

*(Bildschirmfoto folgt: Terminal während des Laufzeitpaket-Downloads.)*

## Schritt 4: KassenSturz installieren

Wechseln Sie in den Ordner mit der Datei und installieren Sie sie (Dateiname wie heruntergeladen; mit der Tab-Taste vervollständigt das Terminal ihn):

```sh
cd ~/Downloads
flatpak install --user KassenSturz.flatpak
```

Bestätigen Sie mit **J**. Danach finden Sie KassenSturz auch im **Anwendungsmenü**. Erscheint es dort nicht gleich, melden Sie sich einmal ab und wieder an oder starten Sie es wie in Schritt 5.

*(Bildschirmfoto folgt: Terminal mit „Installation abgeschlossen“ und Anwendungsmenü.)*

## Schritt 5: KassenSturz im Entwicklermodus starten

Für den Test soll KassenSturz ein Protokoll mitschreiben (siehe [Tester-Guide](TESTER_GUIDE.de.md), Abschnitt 3). Starten Sie es dafür **so und nicht über das Anwendungsmenü**:

```sh
flatpak run --env=KASSENSTURZ_DEBUG_LOG=1 io.github.vondor.KassenSturz
```

**Prüfen:** Steht oben im Fenster „Entwicklermodus“? Dann läuft er. Lassen Sie das Terminal offen, bis Sie fertig sind.

*(Bildschirmfoto folgt: KassenSturz mit „Entwicklermodus“ im Fenstertitel.)*

## Schritt 6: Das erste Mal öffnen

1. Folgen Sie dem ersten Start. Bietet Ihre Version Beispielbücher an, wählen Sie eines (empfohlen: *Suburban Family*). Sonst legen Sie ein eigenes Testbuch an.
2. Legen Sie ein Konto an und **denken Sie sich einen Kontostand aus**. Tragen Sie keine echten Zahlen ein.
3. Sie sind fertig, wenn Sie die **Übersicht** sehen. Notieren Sie, wie viele Minuten Sie gebraucht haben. Das ist das Ergebnis von Karte 1.

*(Bildschirmfoto folgt: Startbildschirm und Übersicht.)*

## Später: eine neuere Version darüber installieren (Karte 6)

Laden Sie die neuere Datei wie in Schritt 1 herunter und installieren Sie sie über die alte:

```sh
cd ~/Downloads
flatpak install --user --reinstall KassenSturz.flatpak
```

Ihre Bücher bleiben erhalten.

## Wieder entfernen

```sh
flatpak uninstall --user io.github.vondor.KassenSturz
```

Ihre Testbücher bleiben unter `~/.var/app/io.github.vondor.KassenSturz/` liegen, bis Sie sie löschen.

---

> **Stuck? Ask the facilitator.** / **Nicht weiter? Sagen Sie es der Testleitung.** Notieren Sie, bei welchem Schritt Sie waren und was auf dem Bildschirm stand. Die Testleitung installiert nicht für Sie, hilft Ihnen aber mit einem Hinweis weiter.
