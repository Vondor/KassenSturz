# KassenSturz installieren: Windows

*Dieses Blatt führt Sie Klick für Klick durch die Installation. Sie brauchen dafür keine Hilfe und normalerweise keine Administrator-Rechte. Dauer: etwa 10 Minuten. Englische Fassung: [INSTALL_WINDOWS.en.md](INSTALL_WINDOWS.en.md).*

> **Stand für das Team.** Am 3. November 2026 wurde dieses Blatt auf einer Windows-11-VM mit Build 0.261103.18 abgearbeitet (D-542, D-557). Die Bildschirmfotos dieses Laufs liegen in [screenshots/windows/](screenshots/windows/). Dateinamen, Ordner und Abläufe unten stammen aus diesem Lauf. Ausnahme ist der Installer: Er installiert inzwischen ohne Administrator-Rechte und auf Deutsch (Q-406). Seine Bildschirmfotos und die Korrekturen aus dem Lauf (M6-B141 bis B143, B147, B149) werden in einem zweiten Lauf geprüft. Erst danach dürfen Tester mit Windows eingeladen werden.

**Kommen Sie nicht weiter?** Sagen Sie es der Testleitung. Sie installiert nicht für Sie, aber sie notiert die Stelle, und Sie bekommen einen Hinweis, damit der Test weitergeht.

---

## Was Sie brauchen

- Windows 11 (oder Windows 10), ein normales Benutzerkonto reicht.
- Eine Internetverbindung für den Download.
- Etwa 600 MB freien Platz.

## Schritt 1: Die Download-Seite öffnen

1. Öffnen Sie Ihren Browser (Edge, Firefox oder Chrome).
2. Geben Sie oben in die Adresszeile diese Adresse ein und drücken Sie die Eingabetaste:

   **https://github.com/Vondor/KassenSturz/releases**

3. Sie sehen eine Liste von Versionen. Die oberste ist die neueste. Oben auf der Karte steht **Pre-release** (Testversion). Das ist richtig so.
4. Ein GitHub-Konto brauchen Sie **nicht**. Wenn die Seite danach fragt, schließen Sie die Frage.

*(Bildschirmfoto folgt: Download-Seite mit der obersten Version.)*

## Schritt 2: Die richtige Datei wählen

1. Scrollen Sie auf der Karte zu **Assets** (Dateien) und klappen Sie die Liste auf, falls nötig.
2. Wählen Sie die Datei, deren Name mit **-windows-x64-setup.exe** endet, zum Beispiel `KassenSturz-0.261103.18-windows-x64-setup.exe`. Die Zahl in der Mitte ist die Nummer der Testversion; die Testleitung hat sie Ihnen vorher gesagt.
   Die Datei, die auf **-windows-x64.zip** endet, ist dasselbe Programm zum Entpacken ohne Installation. Nehmen Sie sie nur, wenn die Testleitung es sagt.
3. **Nicht** diese Dateien: *Source code (zip)*, *Source code (tar.gz)* und die Datei, die auf **.flatpak** endet. Die sind für andere Systeme oder für Entwickler.
4. Klicken Sie auf den Namen der Datei. Der Download startet.

*(Bildschirmfoto folgt: Dateiliste, richtige Datei markiert.)*

## Schritt 3: Windows meldet eine Warnung beim Download

Weil KassenSturz noch ein Test ist, kennt Windows die Datei noch nicht. Das zeigt sich in zwei Warnungen. Beide sind bei einer Testversion normal. **Die Datei kommt von der Seite, die Sie eben selbst geöffnet haben.**

**a) Der Browser fragt, ob er die Datei behalten soll.** In Edge erscheint zum Beispiel „… wird nicht häufig heruntergeladen“.
1. Klicken Sie neben der Meldung auf die drei Punkte **…**
2. Klicken Sie auf **Beibehalten**.
3. Klicken Sie auf **Trotzdem beibehalten** (oder **Mehr anzeigen**, dann **Trotzdem beibehalten**).

*(Bildschirmfoto folgt: Download-Warnung im Browser.)*

**b) Beim Öffnen erscheint „Der Computer wurde durch Windows geschützt“ (SmartScreen).**
1. Klicken Sie im blauen Fenster auf den Text **Weitere Informationen**. (Der Knopf „Nicht ausführen“ ist hier der falsche.)
2. Es erscheint ein neuer Knopf **Trotzdem ausführen**. Klicken Sie darauf.

Diese Warnung kommt, weil das Programm noch nicht signiert ist. Das ändert sich später (Microsoft Store und signierter Installer). Für den Test bedeutet es: Sie können die Datei bedenkenlos öffnen, wenn sie von der Seite oben stammt.

*(Bildschirmfoto folgt: SmartScreen-Fenster vor und nach „Weitere Informationen“.)*

## Schritt 4: Installieren

1. Öffnen Sie die heruntergeladene Datei per Doppelklick (im Browser über „Datei öffnen“ oder im Ordner **Downloads**).
2. Folgen Sie dem Installationsfenster: **Weiter**, dann **Installieren**. Sie müssen nichts umstellen. KassenSturz kommt in Ihren eigenen Ordner (`%LOCALAPPDATA%\Programs\KassenSturz`), deshalb braucht die Installation keine Administrator-Rechte.
3. Fragt Windows trotzdem nach einem **Administrator-Kennwort** oder ob die App Änderungen vornehmen darf, wählen Sie **Abbrechen** bzw. **Nein** und sagen Sie es der Testleitung.
4. Am Ende klicken Sie **Fertig stellen**. Im Startmenü gibt es jetzt den Eintrag **KassenSturz**.

*(Bildschirmfoto folgt: Installationsfenster, Schritte.)*

> **Nur falls die Installation „Entwicklermodus“ von Windows verlangt.** Die Testleitung sagt Ihnen vorher, ob das für Ihre Version nötig ist. In der Regel ist es **nicht** nötig.
>
> ⚠️ **Hinweis zum Windows-Entwicklermodus.** Mit dieser Einstellung lässt Windows auch Programme zu, die nicht aus dem Store kommen und nicht geprüft sind. Installieren Sie in dieser Zeit **nur die KassenSturz-Datei von der Seite oben** und nichts anderes. **Schalten Sie ihn nach der Installation wieder aus**; KassenSturz läuft danach weiter.
>
> 1. *Einstellungen › System › Für Entwickler* (Windows 10: *Update und Sicherheit › Für Entwickler*) öffnen und **Entwicklermodus** einschalten. Die Warnung von Windows mit **Ja** bestätigen.
> 2. Die KassenSturz-Datei installieren (Schritt 4).
> 3. An derselben Stelle den **Entwicklermodus wieder ausschalten**.
>
> Das ist etwas anderes als der *KassenSturz*-Entwicklermodus in Schritt 5.

## Schritt 5: KassenSturz im Entwicklermodus starten

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

## Schritt 6: Das erste Mal öffnen

1. Beim ersten Start fragt KassenSturz nach Ihrer Währung. Lassen Sie **EUR** stehen und klicken Sie **Weiter**.
2. Wählen Sie **Ein Beispielbuch ausprobieren** und dann **Vorstadtfamilie (empfohlener Einstieg)**. Das Beispielbuch ist eine ausgedachte Familie. KassenSturz legt daraus ein neues Buch an. Sie können auch ein eigenes Testbuch anlegen.
3. Legen Sie ein Konto an und **denken Sie sich einen Kontostand aus**. Tragen Sie keine echten Zahlen ein.
4. Sie sind fertig, wenn Sie die **Übersicht** sehen. Notieren Sie, wie viele Minuten Sie gebraucht haben. Das ist das Ergebnis von Karte 1.

*(Bildschirmfoto folgt: Startbildschirm und Übersicht.)*

## Wieder entfernen

*Einstellungen › Apps › Installierte Apps*, KassenSturz suchen, **…** und **Deinstallieren**. Die Deinstallation entfernt nur die Dateien des Programms. Ihre Testbücher bleiben als Dateien liegen, bis Sie sie löschen (in `%LOCALAPPDATA%\io.github.vondor.KassenSturz`).

---

> **Stuck? Ask the facilitator.** / **Nicht weiter? Sagen Sie es der Testleitung.** Notieren Sie, bei welchem Schritt Sie waren und was auf dem Bildschirm stand. Die Testleitung installiert nicht für Sie, hilft Ihnen aber mit einem Hinweis weiter.
