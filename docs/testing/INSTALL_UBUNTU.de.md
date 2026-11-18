# KassenSturz installieren: Ubuntu

*Dieses Blatt führt Sie Schritt für Schritt durch die Installation. Sie brauchen dafür keine Hilfe. Ihr Computer-Passwort wird einmal gebraucht. Dauer: etwa 10 bis 15 Minuten. Englische Fassung: [INSTALL_UBUNTU.en.md](INSTALL_UBUNTU.en.md).*

> **Stand für das Team.** Unter Linux kommt KassenSturz über den Snap Store (D-765). Die Beta liegt dort im Kanal *beta*, darum steht Weg A an erster Stelle. Weg B, die Flatpak-Datei von der Download-Seite, ist mit den Befehlen aus der [README](../../README.md) (Abschnitt „Trying a build from CI“) abgeglichen, und bei jeder Version führt die Veröffentlichung (`.github/workflows/release.yml`, [RELEASING.md](../RELEASING.md)) seine Schritte sowie „Wieder entfernen“ auf einem frischen Ubuntu aus. Bildschirmfotos von einem frischen Ubuntu werden nachgetragen; die Stellen sind mit „Bildschirmfoto folgt“ gekennzeichnet (Liste in [README.md](README.md)).

**Kommen Sie nicht weiter?** Schreiben Sie ins [Installationsprotokoll](INSTALL_LOG.de.md), bei welchem Schritt Sie waren und was auf dem Bildschirm stand. Hören Sie dort auf und schicken Sie das Protokoll an die Testleitung. Sie bekommen dann per E-Mail einen Hinweis, damit der Test weitergeht.

---

## Was Sie brauchen

- Ubuntu (aktuelle Version mit KDE oder GNOME).
- Eine Internetverbindung für die Einrichtung.
- Etwa 3 GB freien Platz.
- Ihr Computer-Passwort, einmal.

**Wie Sie Befehle eingeben:** Drücken Sie **Strg + Alt + T**. Es öffnet sich ein Fenster mit Text, das **Terminal**. Kopieren Sie jede Zeile unten dorthin (Einfügen mit **Strg + Umschalt + V**) und drücken Sie die Eingabetaste. Warten Sie, bis wieder eine Eingabezeile erscheint, bevor Sie den nächsten Befehl eingeben.

**Es gibt zwei Wege.** Nehmen Sie **Weg A**. Nur wenn Weg A nicht klappt, nehmen Sie **Weg B**. Danach geht es für beide Wege mit „Das erste Mal öffnen“ weiter.

## Weg A: Aus dem Snap Store (empfohlen)

### Schritt A1: KassenSturz installieren (braucht Ihr Passwort)

Im Terminal:

```sh
sudo snap install kassensturz --beta
```

1. Es fragt nach **Ihrem Passwort**. Beim Tippen erscheint nichts auf dem Bildschirm, das ist normal. Passwort eintippen, Eingabetaste.
2. Der Download dauert einige Minuten. Am Ende steht eine Zeile mit `kassensturz` und der Versionsnummer, zum Beispiel `0.261117.8`. Die Testleitung hat Ihnen die Nummer vorher gesagt.
3. Steht dort „snap: Befehl nicht gefunden“ (oder „command not found“), nehmen Sie Weg B.

*(Bildschirmfoto folgt: Terminal nach der Installation.)*

### Schritt A2: KassenSturz im Entwicklermodus starten

Für den Test soll KassenSturz ein Protokoll mitschreiben (siehe [Tester-Guide](TESTER_GUIDE.de.md), Abschnitt 3). Starten Sie es dafür **so und nicht über das Anwendungsmenü**:

```sh
KASSENSTURZ_DEBUG_LOG=1 kassensturz
```

**Prüfen:** Steht oben im Fenster „Entwicklermodus“? Dann läuft er. Lassen Sie das Terminal offen, bis Sie fertig sind. Weiter mit „Das erste Mal öffnen“.

*(Bildschirmfoto folgt: KassenSturz mit „Entwicklermodus“ im Fenstertitel.)*

## Weg B: Mit der Flatpak-Datei (nur wenn Weg A nicht klappt)

### Schritt B1: Die Download-Seite öffnen

1. Öffnen Sie den Browser und geben Sie in die Adresszeile ein:

   **https://github.com/Vondor/KassenSturz/releases**

2. Die oberste Karte ist die neueste Beta-Version. Oben steht **Pre-release**. Das ist richtig so. Ein GitHub-Konto brauchen Sie nicht.
3. Scrollen Sie zu **Assets** (Dateien) und laden Sie die Datei herunter, die auf **.flatpak** endet. Sie heißt zum Beispiel `KassenSturz-0.261117.8-x86_64.flatpak`; die Zahl in der Mitte ist die Versionsnummer, und die Testleitung hat Ihnen die richtige vorher gesagt. **Nicht** *Source code (zip)*, nicht die Windows-Dateien und nicht `kassensturz.snap`.
4. Die Datei liegt danach im Ordner **Downloads**.

*(Bildschirmfoto folgt: Download-Seite mit der richtigen Datei.)*

### Schritt B2: Flatpak einrichten (einmal, braucht Ihr Passwort)

Flatpak ist die Art, wie KassenSturz unter Ubuntu installiert wird. Ubuntu bringt es nicht immer mit.

1. Im Terminal:

   ```sh
   sudo apt install flatpak
   ```

2. Es fragt nach **Ihrem Passwort**. Beim Tippen erscheint nichts auf dem Bildschirm, das ist normal. Passwort eintippen, Eingabetaste.
3. Fragt es „Möchten Sie fortfahren? [J/n]“, tippen Sie **J** und die Eingabetaste.

*(Bildschirmfoto folgt: Terminal mit der Passwortabfrage.)*

### Schritt B3: Den Software-Katalog und das KDE-Laufzeitpaket hinzufügen (ohne Passwort, nur für Sie)

KassenSturz braucht ein Laufzeitpaket von KDE. Beide Befehle brauchen **kein** Passwort und ändern nur Ihr eigenes Benutzerkonto:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.kde.Platform//6.11
```

Beim zweiten Befehl fragt Flatpak ein- oder zweimal nach einer Bestätigung. Tippen Sie jeweils **J** und die Eingabetaste. Der Download dauert einige Minuten. Warten Sie, bis wieder eine Eingabezeile da ist.

*(Bildschirmfoto folgt: Terminal während des Laufzeitpaket-Downloads.)*

### Schritt B4: KassenSturz installieren

Wechseln Sie in den Ordner mit der Datei und installieren Sie sie (Tippen Sie `flatpak install --user KassenSturz-` und drücken Sie die **Tab-Taste**: das Terminal ergänzt den Rest des Dateinamens. Liegen mehrere Dateien im Ordner, nehmen Sie die mit der Versionsnummer, die die Testleitung genannt hat):

```sh
cd ~/Downloads
flatpak install --user KassenSturz-0.261117.8-x86_64.flatpak
```

Bestätigen Sie mit **J**. Danach finden Sie KassenSturz auch im **Anwendungsmenü**. Erscheint es dort nicht gleich, melden Sie sich einmal ab und wieder an oder starten Sie es wie in Schritt B5.

*(Bildschirmfoto folgt: Terminal mit „Installation abgeschlossen“ und Anwendungsmenü.)*

### Schritt B5: KassenSturz im Entwicklermodus starten

Für den Test soll KassenSturz ein Protokoll mitschreiben (siehe [Tester-Guide](TESTER_GUIDE.de.md), Abschnitt 3). Starten Sie es dafür **so und nicht über das Anwendungsmenü**:

```sh
flatpak run --env=KASSENSTURZ_DEBUG_LOG=1 io.github.vondor.KassenSturz
```

**Prüfen:** Steht oben im Fenster „Entwicklermodus“? Dann läuft er. Lassen Sie das Terminal offen, bis Sie fertig sind. Weiter mit „Das erste Mal öffnen“.

*(Bildschirmfoto folgt: KassenSturz mit „Entwicklermodus“ im Fenstertitel.)*

## Das erste Mal öffnen (beide Wege)

1. Auf der ersten Seite **Willkommen bei KassenSturz** wählen Sie Ihre Sprache und Währung. Meist ist schon alles richtig eingestellt; klicken Sie **Weiter**.
2. Auf der Seite **Wie möchten Sie anfangen?** wählen Sie **Ein Beispielbuch ausprobieren** und dann die *Vorstadtfamilie*. Mit diesem Buch arbeiten Sie auch im Termin. Tragen Sie keine echten Zahlen ein.
3. Sie sind fertig, wenn Sie die **Übersicht** sehen. Notieren Sie im [Installationsprotokoll](INSTALL_LOG.de.md), wie viele Minuten Sie insgesamt gebraucht haben.

*(Bildschirmfoto folgt: Die Seiten des Einrichtungsassistenten und die Übersicht.)*

## Wieder entfernen

**Weg A (Snap Store):**

```sh
sudo snap remove kassensturz
```

**Achtung:** Dabei löscht Snap auch die Daten der App, also Ihr Testbuch und die Sicherungen in `~/snap/kassensturz`. Snap legt zwar meist eine Sicherung („Snapshot“) an, verlassen Sie sich aber nicht darauf. Für den Test ist das unproblematisch (Beispielbuch). **Schicken Sie das Protokoll deshalb vorher an die Testleitung.** Wollen Sie ein Buch behalten, kopieren Sie es vorher an einen anderen Ort.

**Weg B (Flatpak):**

```sh
flatpak uninstall --user io.github.vondor.KassenSturz
```

Bei Weg B bleiben Ihre Testbücher unter `~/.var/app/io.github.vondor.KassenSturz/` liegen, bis Sie sie löschen.

---

> **Nicht weiter?** Notieren Sie im [Installationsprotokoll](INSTALL_LOG.de.md), bei welchem Schritt Sie waren und was auf dem Bildschirm stand, und schicken Sie es an die Testleitung. Sie bekommen dann per E-Mail einen Hinweis.
