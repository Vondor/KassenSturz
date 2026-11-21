<!-- Nach Auslieferung von D-856 (#1178, PR #1180) den Abschnitt „Wieder entfernen“ anpassen: Buch und Sicherungen liegen dann in Dokumente\KassenSturz und bleiben nach dem Entfernen; Einstellungen und Protokoll bleiben im Paket; ist Dokumente mit OneDrive verbunden, wird das Buch dorthin synchronisiert. Der Abschnitt „Das Protokoll finden“ entfällt mit B-381 (#1177), sobald der Knopf „Protokoll zeigen“ den Ordner öffnet. -->
# KassenSturz installieren: Windows

*Dieses Blatt führt Sie Klick für Klick durch die Installation. Sie brauchen dafür keine Hilfe und normalerweise keine Administrator-Rechte. Dauer: etwa 10 Minuten. Englische Fassung: [INSTALL_WINDOWS.en.md](INSTALL_WINDOWS.en.md).*


**Kommen Sie nicht weiter?** Schreiben Sie ins [Installationsprotokoll](INSTALL_LOG.de.md), bei welchem Schritt Sie waren und was auf dem Bildschirm stand. Hören Sie dort auf und schicken Sie das Protokoll an die Testleitung. Sie bekommen dann per E-Mail einen Hinweis, damit der Test weitergeht.

---

## Was Sie brauchen

- Windows 11 (oder Windows 10), ein normales Benutzerkonto reicht.
- Eine Internetverbindung für den Download.
- Für Weg A: Zugang zum Microsoft Store (auf Windows 11 und 10 vorhanden; ein Microsoft-Konto ist meist nicht nötig).
- Etwa 600 MB freien Platz.

## Weg A: Aus dem Microsoft Store (empfohlen)

### Schritt A1: KassenSturz im Store öffnen

1. Öffnen Sie Ihren Browser und geben Sie diese Adresse ein:

   **https://apps.microsoft.com/detail/9NM5V48PPRLQ**

2. Es öffnet sich die Seite von KassenSturz im Microsoft Store. Dort heißt es **„KassenSturz - The Household Budget App“**. Bitte nehmen Sie diesen Link und suchen Sie nicht im Internet nach dem Namen: Dort erscheint zuerst etwas ganz anderes. Fragt der Browser, ob er die App **Microsoft Store** öffnen darf, wählen Sie **Öffnen**.

*(Bildschirmfoto folgt: KassenSturz im Microsoft Store.)*

### Schritt A2: Installieren

1. Klicken Sie auf **Installieren** (manchmal heißt der Knopf **Herunterladen**).
2. Warten Sie, bis der Store fertig ist. Sie brauchen dafür keine Administrator-Rechte, und es erscheint keine Sicherheitswarnung.
3. Im Startmenü gibt es jetzt den Eintrag **KassenSturz**.

### Schritt A3: KassenSturz im Entwicklermodus starten

Für den Test soll KassenSturz ein Protokoll mitschreiben (siehe [Tester-Guide](TESTER_GUIDE.de.md), Abschnitt 3). Dafür schalten Sie den Entwicklermodus einmal ein:

1. Drücken Sie die **Windows-Taste**, tippen Sie **PowerShell** und drücken Sie die Eingabetaste.
2. Kopieren Sie diese Zeile in das Fenster und drücken Sie die Eingabetaste:

   ```powershell
   [Environment]::SetEnvironmentVariable("KASSENSTURZ_DEBUG_LOG", "1", "User")
   ```

3. Schließen Sie PowerShell. Starten Sie KassenSturz über das **Startmenü** (Eintrag **KassenSturz**).
4. **Prüfen:** Steht oben im Fenster „KassenSturz – Entwicklermodus“ und darunter ein roter Hinweis? Dann läuft er.

**Nach dem Test** schalten Sie den Entwicklermodus wieder aus, mit derselben Zeile, aber `$null` statt `"1"`:

```powershell
[Environment]::SetEnvironmentVariable("KASSENSTURZ_DEBUG_LOG", $null, "User")
```

*(Bildschirmfoto folgt: KassenSturz mit „Entwicklermodus“ im Fenstertitel.)*

Weiter mit **Das erste Mal öffnen** unten.

## Weg B: Mit der Installationsdatei (nur wenn Weg A nicht klappt)

Nehmen Sie diesen Weg nur, wenn der Store bei Ihnen nicht geht (zum Beispiel weil er auf Ihrem Computer gesperrt ist). Notieren Sie im Protokoll, warum.

### Schritt B1: Die Download-Seite öffnen

1. Öffnen Sie Ihren Browser (Edge, Firefox oder Chrome).
2. Geben Sie oben in die Adresszeile diese Adresse ein und drücken Sie die Eingabetaste:

   **https://github.com/Vondor/KassenSturz/releases**

3. Sie sehen eine Liste von Versionen. Die oberste ist die neueste Beta-Version. Oben auf der Karte steht **Pre-release** (oder **Latest**, wenn sie zugleich die stabile Ausgabe ist). Das ist richtig so.
4. Ein GitHub-Konto brauchen Sie **nicht**. Wenn die Seite danach fragt, schließen Sie die Frage.

*(Bildschirmfoto folgt: Download-Seite mit der obersten Version.)*

### Schritt B2: Die richtige Datei wählen

1. Scrollen Sie auf der Karte zu **Assets** (Dateien) und klappen Sie die Liste auf, falls nötig.
2. Wählen Sie die Datei, deren Name mit **-windows-x64-setup.exe** endet, zum Beispiel `KassenSturz-0.261117.8-windows-x64-setup.exe`. Die Zahl in der Mitte ist die Versionsnummer; die Testleitung hat sie Ihnen vorher gesagt.
   Die Datei, die auf **-windows-x64.zip** endet, ist dasselbe Programm zum Entpacken ohne Installation. Nehmen Sie sie nur, wenn die Testleitung es sagt.
3. **Nicht** diese Dateien: die Datei, die auf **.msix** endet (sie ist für den Microsoft Store), *Source code (zip)*, *Source code (tar.gz)*, die Datei, die auf **.flatpak** endet, und `kassensturz.snap`. Die sind für andere Wege, andere Systeme oder für Entwickler.
4. Klicken Sie auf den Namen der Datei. Der Download startet.

*(Bildschirmfoto folgt: Dateiliste, richtige Datei markiert.)*

### Schritt B3: Windows meldet eine Warnung beim Download

Weil KassenSturz noch ein Test ist, kennt Windows die Datei noch nicht. Das zeigt sich in zwei Warnungen. Beide sind bei einer Beta-Version normal. **Die Datei kommt von der Seite, die Sie eben selbst geöffnet haben.**

**a) Der Browser fragt, ob er die Datei behalten soll.** In Edge erscheint zum Beispiel „… wird nicht häufig heruntergeladen“.
1. Klicken Sie neben der Meldung auf die drei Punkte **…**
2. Klicken Sie auf **Beibehalten**.
3. Klicken Sie auf **Trotzdem beibehalten** (oder **Mehr anzeigen**, dann **Trotzdem beibehalten**).

*(Bildschirmfoto folgt: Download-Warnung im Browser.)*

**b) Beim Öffnen erscheint „Der Computer wurde durch Windows geschützt“ (SmartScreen).**
1. Klicken Sie im blauen Fenster auf den Text **Weitere Informationen**. (Der Knopf „Nicht ausführen“ ist hier der falsche.)
2. Es erscheint ein neuer Knopf **Trotzdem ausführen**. Klicken Sie darauf.

Diese Warnung kommt, weil das Programm noch nicht signiert ist. Über den Microsoft Store (Weg A) kommt diese Warnung nicht. Für den Test bedeutet es: Sie können die Datei bedenkenlos öffnen, wenn sie von der Seite oben stammt.

*(Bildschirmfoto folgt: SmartScreen-Fenster vor und nach „Weitere Informationen“.)*

### Schritt B4: Installieren

1. Öffnen Sie die heruntergeladene Datei per Doppelklick (im Browser über „Datei öffnen“ oder im Ordner **Downloads**).
2. Folgen Sie dem Installationsfenster: **Weiter**, dann **Installieren**. Sie müssen nichts umstellen. KassenSturz kommt in Ihren eigenen Ordner (`%LOCALAPPDATA%\Programs\KassenSturz`), deshalb braucht die Installation keine Administrator-Rechte.
3. Fragt Windows trotzdem nach einem **Administrator-Kennwort** oder ob die App Änderungen vornehmen darf, wählen Sie **Abbrechen** bzw. **Nein** und notieren Sie es im Installationsprotokoll.
4. Am Ende klicken Sie **Fertig stellen**. Im Startmenü gibt es jetzt den Eintrag **KassenSturz**.

*(Bildschirmfoto folgt: Installationsfenster, Schritte.)*

### Schritt B5: KassenSturz im Entwicklermodus starten

Für den Test soll KassenSturz ein Protokoll mitschreiben (siehe [Tester-Guide](TESTER_GUIDE.de.md), Abschnitt 3). Dafür starten Sie es einmal so:

1. Drücken Sie die **Windows-Taste**, tippen Sie **PowerShell** und drücken Sie die Eingabetaste. Ein Fenster mit Text öffnet sich.
2. Kopieren Sie diese Zeile in das Fenster und drücken Sie die Eingabetaste:

   ```powershell
   $env:KASSENSTURZ_DEBUG_LOG = "1"; & "$env:LOCALAPPDATA\Programs\KassenSturz\bin\kassensturz.exe"
   ```

3. **Beim allerersten Start brauchen Sie Geduld: Es kann bis zu 7 Minuten dauern, bis sich das Fenster öffnet.** Windows prüft dabei jede Datei des neuen Programms einmal. Starten Sie KassenSturz in dieser Zeit nicht noch einmal. Ab dem zweiten Start dauert es nur wenige Sekunden.
4. KassenSturz öffnet sich. **Prüfen:** Steht oben im Fenster „KassenSturz – Entwicklermodus“ und darunter ein roter Hinweis? Dann läuft er.

Lassen Sie das PowerShell-Fenster offen, bis Sie fertig sind.

*(Bildschirmfoto folgt: PowerShell mit dem Befehl; KassenSturz mit „Entwicklermodus“ im Fenstertitel.)*

## Das erste Mal öffnen (beide Wege)

1. Auf der ersten Seite **Willkommen bei KassenSturz** wählen Sie Ihre Sprache und Währung. Meist ist schon alles richtig eingestellt; klicken Sie **Weiter**.
2. Auf der Seite **Wie möchten Sie anfangen?** wählen Sie **Ein Beispielbuch ausprobieren** und dann die *Vorstadtfamilie*. Mit diesem Buch arbeiten Sie auch im Termin.
3. Sie sind fertig, wenn Sie die **Übersicht** sehen. Notieren Sie im [Installationsprotokoll](INSTALL_LOG.de.md), wie viele Minuten Sie insgesamt gebraucht haben. 

*(Bildschirmfoto folgt: Die Seiten des Einrichtungsassistenten und die Übersicht.)*

## Das Protokoll finden (für Ihren Bericht)

Der Knopf **Protokoll zeigen** im roten Hinweis öffnet bei der Store-Version derzeit nicht den richtigen Ordner. So finden Sie das Protokoll trotzdem:

1. Drücken Sie **Windows-Taste + R**, kopieren Sie diese Zeile in das Feld und drücken Sie die Eingabetaste:

   ```
   %LOCALAPPDATA%\Packages\47425SerpentSoftware.KassenSturz-TheHouseholdBudge_6gxp6ch7ybyne\LocalCache\Local\io.github.vondor.KassenSturz\debug-log
   ```

2. Es öffnet sich ein Ordner. Die Datei **current.jsonl** ist das Protokoll. Schicken Sie sie mit Ihrem Bericht an die Testleitung.

Bei Weg B liegt das Protokoll in `%LOCALAPPDATA%\io.github.vondor.KassenSturz\debug-log`.

## Wieder entfernen

- **Weg A (Store):** Startmenü öffnen, „Kassen“ tippen, beim Eintrag **KassenSturz** auf **Optionen** (oder rechte Maustaste) und **Deinstallieren**. **Achtung:** Dabei löscht Windows den ganzen Datenordner der App, mit dem Buch an seinem Standardplatz, allen Sicherungen und dem Protokoll. **Schicken Sie das Protokoll deshalb vorher an die Testleitung.** Für den Test ist das sonst unproblematisch (Beispielbuch).
- **Weg B (Installationsdatei):** *Einstellungen › Apps › Installierte Apps*, KassenSturz suchen, **…** und **Deinstallieren**. Ihre Testbücher bleiben als Dateien liegen, bis Sie sie löschen (in `%LOCALAPPDATA%\io.github.vondor.KassenSturz`).

---

> **Nicht weiter?** Notieren Sie im [Installationsprotokoll](INSTALL_LOG.de.md), bei welchem Schritt Sie waren und was auf dem Bildschirm stand, und schicken Sie es an die Testleitung. Sie bekommen dann per E-Mail einen Hinweis.
