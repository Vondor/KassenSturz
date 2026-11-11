# 11. Mit der Bank verbinden

*KassenSturz kann bei vielen deutschen Banken neue Buchungen selbst abrufen. Diese Seite zeigt, wie Sie einen Bankzugang einrichten und neue Buchungen holen. KassenSturz liest nur; es kann nie Geld senden.*

> **Stand der Testversion:** Die Bankverbindung wird noch geprüft. Je nach Version fehlt sie im Einrichtungsassistenten; unter **Einstellungen › Bankverbindungen** steht sie immer. Wer lieber auf Nummer sicher geht, liest Dateien ein (Seite 10).

## Was geht und was nicht

- KassenSturz ruft den Kontostand und die gebuchten Umsätze von Girokonten, Sparkonten und Kreditkarten ab. Depots listet es als „noch nicht unterstützt“.
- Es nutzt **FinTS**, den Standard der deutschen Banken. Für Konten im Ausland lesen Sie Dateien ein.
- Es gibt **keine** Funktion, die Überweisungen, Daueraufträge oder Lastschriften auslöst.
- Die Verbindung läuft über **AqBanking**, ein freies Programm. Das Flatpak und die Windows-Version bringen es mit.

## Einen Bankzugang einrichten

1. **Einstellungen › Bankverbindungen › Eine Bank verbinden…**
2. Tippen Sie die **IBAN** eines Ihrer Konten bei dieser Bank. KassenSturz findet die Bank und fragt nur, was sie braucht: meist Ihren **Anmeldenamen**, wo es das gibt eine **Kundenkennung**, und Ihre **PIN**.
3. Wählen Sie bei **PIN merken**, wie lange KassenSturz sie behält (unten).
4. Fragt die Bank nach einer **TAN** oder nach Ihrer **Freigabe in der App**, zeigt KassenSturz die Frage der Bank mit deren Knöpfen in einem eigenen Fenster. Eine TAN geht einmal an die Bank und wird nie gespeichert. Schließen Sie das Fenster, brechen Sie ab, und nichts Halbfertiges bleibt zurück.
5. Zum Schluss listet KassenSturz die Konten, die Ihre Bank zeigt, und schlägt vor, welches Konto in Ihrem Buch jeweils gefüllt wird. Sie bestätigen oder ändern jede Wahl.

Kennt KassenSturz Ihre Bank oder deren Adresse nicht, können Sie die **FinTS-Adresse** selbst eintragen; sie muss mit `https://` beginnen. Bei Genossenschaftsbanken, die in den Listen fehlen, setzt KassenSturz die Adresse der Nachbarn ein und sagt, dass es geraten ist. Prüfen Sie sie, wenn die Verbindung scheitert.

Eine TAN, die die Bank als Bild zeigt (Flickercode, photoTAN, QR-Code), kann KassenSturz noch nicht anzeigen. Es sagt das, bevor es etwas sendet. Wählen Sie bei Ihrer Bank ein TAN-Verfahren, das als Text kommt oder in der App freigibt.

## Wohin kommt Ihre PIN?

Unter **PIN merken** haben Sie drei Möglichkeiten:

- **jedes Mal tippen** (Voreinstellung),
- **bis Sie KassenSturz schließen**,
- **im Passwortspeicher Ihres Computers** (KWallet, „Passwörter und Schlüssel“ oder die Anmeldeinformationsverwaltung von Windows).

Unter **Einstellungen › Passwortspeicher** legen Sie fest, wo gemerkte PINs liegen; die Voreinstellung (Systemspeicher) ist für fast alle richtig. KassenSturz schreibt eine PIN **nirgends sonst** hin, nicht ins Buch und in keine Datei. Sagt Ihre Bank, die PIN sei falsch, **vergisst** KassenSturz sie sofort und fragt einmal nach; nach einer zweiten falschen PIN hört es auf, damit Ihr Zugang nicht gesperrt wird. Mit **PIN vergessen** und **Bankzugang und gespeicherte PIN entfernen…** räumen Sie später auf. Ihre Konten und Buchungen bleiben dabei erhalten.

## Neue Buchungen abrufen

1. Wählen Sie **Neue Buchungen abrufen** auf der Kontenseite oder in der Liste eines Kontos.
2. Jedes Konto zeigt sein Bankkonto und den Tag der letzten Abfrage oder **Mit einem Bankkonto verknüpfen…**.
3. Haken Sie die Konten an und wählen Sie wieder **Neue Buchungen abrufen**. KassenSturz fragt nacheinander nach dem Neuen; die PIN nur, wenn es sie nicht kennt.
4. **Es wird noch nichts hinzugefügt.** Sie sehen dieselbe Vorschau wie bei einer Datei (Seite 10) und entscheiden. Erst **Importieren** fügt die Buchungen hinzu, und **Rückgängig** nimmt sie samt dem Abruftag zurück.

Eine Abfrage beginnt fünf Tage vor der letzten, weil Banken manchmal einen Tag zu spät buchen; Doppeltes erkennt KassenSturz.

## Die Erinnerung

Ist ein verknüpftes Konto länger nicht abgerufen worden, warnt die Übersicht mit dem Kontonamen, dem Tag und der Zahl der Tage. Banken zeigen nur ihre jüngsten Umsätze, ein langes Warten kann Lücken lassen. Unter **Einstellungen › Erinnerung an neue Buchungen** wählen Sie 30, 60 (Voreinstellung), 90 oder 180 Tage oder „Nie erinnern“. KassenSturz meldet sich nie von selbst bei Ihrer Bank.

## Was gespeichert wird

Das Buch merkt sich nur, welches Bankkonto welches Konto füllt (nach IBAN) und den Tag der letzten Abfrage. **Kein Anmeldename, keine PIN** steht im Buch, in Sicherungen oder in einer gespeicherten Kopie.

**Wenn etwas anders aussieht:** Sagt eine Zeile unter dem Konto, was schiefging, zum Beispiel dass die Bank nicht erreichbar oder die PIN falsch war, folgen Sie dem Hinweis dort. Fragen Sie nie jemanden nach Ihrer PIN, auch uns nicht.
