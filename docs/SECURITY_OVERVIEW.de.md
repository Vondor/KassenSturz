# Wie sicher ist KassenSturz?

*Für alle, die wissen möchten, ob sie KassenSturz ihre Kontodaten anvertrauen können. Ohne Fachwörter. Diese Seite gibt es auch [auf Englisch](SECURITY_OVERVIEW.en.md).*

Kurz gesagt: Ihre Daten bleiben auf Ihrem Computer, KassenSturz liest bei der Bank nur und überweist nie etwas, und wir greifen die App selbst gezielt an, um Fehler zu finden, bevor Sie es tun. KassenSturz ist aber noch in einer frühen Testphase. Weiter unten steht ehrlich, was es noch **nicht** kann.

## Wo Ihre Daten liegen

- **Auf Ihrem Computer, nirgends sonst.** Ihr Haushaltsbuch ist eine Datei auf Ihrem Computer. Es gibt keinen Online-Dienst und kein Konto bei uns. Wir sehen Ihre Daten nie.
- **Keine heimlichen Verbindungen.** KassenSturz sendet keine Nutzungsdaten, keine Absturzberichte und keine Werbung. Es baut nur dann eine Verbindung auf, wenn Sie es selbst anstoßen: zu Ihrer Bank, oder zu einer KI, die Sie selbst eingeschaltet und ausgewählt haben.
- **Ihre Daten gehören Ihnen.** Sie können das Buch jederzeit kopieren, exportieren und in einem offenen Format weitergeben.

## Ihre Bank und Ihre PIN

- **Nur lesen, nie überweisen.** KassenSturz holt Kontostände und Umsätze ab. Überweisungen, Daueraufträge oder Lastschriften kann es gar nicht auslösen; das ist absichtlich nicht eingebaut.
- **Ihre PIN steht in keiner Datei.** Sie entscheiden, ob KassenSturz die PIN jedes Mal erfragt, sie bis zum Schließen der App im Arbeitsspeicher behält oder im Passwortspeicher Ihres Systems ablegt. Im Buch, in Sicherungen und in Kopien steht weder PIN noch Anmeldename.
- **Schutz vor Sperrung.** Meldet die Bank eine falsche PIN, vergisst KassenSturz sie sofort und fragt noch einmal. Nach einer zweiten falschen PIN hört es auf, damit Ihr Zugang nicht gesperrt wird.
- **Verschlüsselte Verbindung.** Die Verbindung zur Bank ist verschlüsselt. Passt das Sicherheitszertifikat der Bank nicht, bricht KassenSturz ab. Ein „trotzdem weiter“ gibt es dabei nicht.
- Niemand von KassenSturz fragt Sie jemals nach Ihrer PIN oder TAN.

## Die KI ist freiwillig

- **Ausgeschaltet, bis Sie sie einschalten.** Solange die KI aus ist, sehen Sie nichts von ihr, und es läuft auch nichts von ihr im Hintergrund.
- **Sie wählen, wohin.** Sie können ein KI-Modell auf Ihrem eigenen Computer benutzen. Soll etwas Ihren Computer verlassen, sagt KassenSturz vorher, was gesendet wird, und fragt Sie.
- **Die KI schlägt vor, Sie entscheiden.** Jede Änderung, die eine KI vorbereitet, sehen Sie vorher und bestätigen sie selbst. Große oder löschende Änderungen bekommen eine deutliche Warnung.
- **Zahlen rechnet KassenSturz, nicht die KI.** Kontostände und Summen kommen immer aus dem Programm selbst.

## Schutz vor Datenverlust

- **Alles lässt sich rückgängig machen.** Jede Änderung, auch ein ganzer eingelesener Kontoauszug, lässt sich mit einem Schritt zurücknehmen.
- **Ganz oder gar nicht.** Wird der Computer mitten im Speichern ausgeschaltet, ist danach entweder alles gespeichert oder nichts. Halbe Buchungen gibt es nicht.
- **Sicherung vor jedem Update.** Bevor eine neue Version Ihr Buch umstellt, legt sie eine Sicherungskopie an.
- **Prüfung beim Öffnen.** Ein Buch, in das jemand heimlich eigene Befehle eingebaut hat, erkennt KassenSturz und öffnet es nicht.

## Was wir angegriffen haben

Wir behandeln KassenSturz wie ein Angreifer. Ein Teil der Angriffe ist fest in die automatischen Tests eingebaut und läuft bei jeder Änderung am Programm. Dazu kommen Angriffsrunden in einem abgeschotteten Testcomputer, teils von Hand, teils von KI-Modellen, die die App absichtlich täuschen sollten. Immer mit ausgedachten Beispielbüchern, nie mit echten Daten.

Angegriffen wurde unter anderem:

- **Gefälschte und kaputte Dateien:** Kontoauszüge (MT940, CAMT, OFX, CSV) und KMyMoney-Dateien mit falschen Daten, unmöglichen Beträgen, versteckten Zeichen und absichtlich riesigem Inhalt. Dazu ein Zufallstest mit vielen Millionen verdrehten Dateien und ein Test, der bei über einer Million eingelesener Umsätze jeden Kontostand auf den Cent nachrechnet.
- **Geld:** riesige Beträge, Rundung zwischen Währungen, Aufteilungen, Umbuchungen, geplante Zahlungen und die Vorschau.
- **Abstürze und Stromausfall:** das Programm mitten im Speichern, Einlesen oder Rückgängigmachen abgeschossen, tausende Male; echte Stromausfälle im Testcomputer; volle Festplatte; ein USB-Stick, der plötzlich verschwindet; eine Uhr, die vor- oder zurückspringt.
- **Zwei Programme am selben Buch:** mehrere Fenster und Programme, die gleichzeitig schreiben.
- **Täuschung über Namen:** Konten und Kategorien, die gleich aussehen, aber andere, unsichtbare oder fremde Buchstaben enthalten.
- **Täuschung der KI:** Empfängernamen und Notizen mit versteckten Befehlen an die KI, getestet mit mehreren lokalen KI-Modellen. Keines hat die Befehle ausgeführt, und jede Änderung wartete auf Ihre Bestätigung.
- **„KI aus“ wirklich aus:** ob bei ausgeschalteter KI noch irgendetwas von ihr läuft oder sichtbar ist.
- **Export:** ob ein Name in einer exportierten Tabelle in einer Tabellenkalkulation zu einer gefährlichen Formel werden kann.

## Was wir gefunden und behoben haben

Jeder Fund bekommt eine Nummer (B-…) und wird in einer eigenen Änderung (#…) behoben. Danach greifen wir genau diese Stelle noch einmal an.

| Was passieren konnte | Fund | Behoben in |
|---|---|---|
| Ein absichtlich verändertes Buch konnte beim Bearbeiten Teile von Buchungen löschen, und „Rückgängig“ konnte falsche Beträge zurückschreiben | B-245, B-246 | #883 |
| Eine KI konnte mehr Änderungen auf einmal vorschlagen als erlaubt, und Namen mit fremden, gleich aussehenden Buchstaben fielen nicht auf | B-247, B-250 | #884 |
| Große, löschende KI-Änderungen kamen ohne deutliche Warnung; Regeln einer KI waren nicht als solche gekennzeichnet | B-248, B-249 | #900 |
| Eine Webadresse in einer KI-Antwort erreichte Sie ohne Hinweis | B-251 | #888 |
| Eine von Hand in die Einstellungsdatei geschriebene Zustimmung zählte wie Ihre echte, sodass Daten an eine fremde KI hätten gehen können | B-252 | #897 |
| Ein Gesprächsprotokoll blieb nach dem Ausschalten des Entwicklermodus liegen | B-253 | #885 |
| Sehr große Beträge konnten Kontostände unlesbar machen | B-269 | #979 |
| Eine Umbuchung von einem Konto auf dasselbe Konto wurde angenommen | B-270 | #1002 |
| Bei sehr langen Zeiträumen sagte die Liste der geplanten Zahlungen fälschlich „keine geplant“ | B-271 | #999 |
| Ein unsichtbares Zeichen machte aus „Giro“ ein zweites, gleich aussehendes Konto | B-272 | #1005 |
| Ein als eigenes Zeichen getippter Akzent machte aus „Café“ ein zweites, gleich aussehendes Konto | B-272 | #1016 |
| Eine weit vorgestellte Uhr löschte den ganzen Änderungsverlauf | B-274 | #975 |
| Beim Beenden über das System blieb das Buch als „geöffnet“ markiert | B-276, B-287 | #989, #998 |
| Beim Einlesen wurden zwei echte, gleiche Buchungen (zum Beispiel zwei Kaffees) als eine gezählt | B-277, B-279 | #986 |
| Ein Stromausfall kurz nach dem Speichern konnte die letzte Änderung zurücknehmen | B-278 | #983 |
| Manche Kontoauszüge in anderen Zeichensätzen wurden nicht gelesen | B-280 | #996 |
| Bei manchen CSV-Dateien ging die erste Buchung als Überschrift verloren | B-281 | #984 |
| Ein KMyMoney-Buch mit zwei gleich benannten Konten ließ sich nicht einlesen | B-283 | #997 |
| Ein im Belegordner unbemerkt ausgetauschter Beleg fiel nicht auf | B-284 | #969 |
| Nach einem Zurückwechseln auf eine ältere Version fehlte der Hinweis auf die Sicherung | B-285 | #992 |
| Bei ausgeschalteter KI gab es noch einen unsichtbaren Rest des Chatfensters | B-286 | #991 |
| Zu große Beträge in KI-Vorschlägen wurden erst spät und mit falscher Begründung abgelehnt | B-288 | #994 |
| Ersetzte ein Sync-Programm das offene Buch, war die Meldung unklar | B-289 | #1000 |
| Ein Hinweis zu sehr vielen Berichten war abgeschnitten | B-290 | #1001, #1003 |
| Das Windows-Paket enthielt 131 Programme, die niemand braucht (etwa ffmpeg und openssl), jedes eines mehr, das missbraucht werden könnte | B-292 | #1014 |
| War das Laufwerk voll, endete ein Import mit einer Meldung, die weder sagte, was passiert war, noch was zu tun ist | B-282 | #1007 |
| Ein Buch mit einer veränderten Zahl in der Datei öffnete sich mit falschem Kontostand, eine beschädigte Seite fiel erst beim nächsten Speichern auf | B-291 | #1010 |

Kein Fund hat eine PIN verraten. Nur bei einem (B-252) hätten Daten Ihren Computer verlassen können, und auch das nur, wenn jemand vorher Ihre Einstellungsdatei von Hand gefälscht hätte.

## Was KassenSturz noch nicht kann (ehrlich gesagt)

- **Es ist eine Testversion.** KassenSturz ist in der Alpha-Phase. Bitte benutzen Sie zum Testen ausgedachte Zahlen oder ein Beispielbuch, nicht Ihr echtes Haushaltsbuch.
- **Keine Prüfung durch eine externe Sicherheitsfirma.** KassenSturz wird zum größten Teil von KI-Programmen gebaut und geprüft, unter Aufsicht eines Menschen, der jede wichtige Entscheidung trifft. Eine gezielte zusätzliche Prüfung der heikelsten Teile (Geld, Passwortspeicher, Bank) ist vor der fertigen Version geplant.
- **Das Buch ist nicht verschlüsselt.** Es enthält keine PIN, aber Ihre Buchungen. Wer eine Kopie der Datei bekommt, kann sie lesen. Schützen Sie Ihren Computer mit einem Passwort und, wenn möglich, mit verschlüsselter Festplatte; Gleiches gilt für Sicherungen und Sync-Ordner.
- **Schadprogramme auf Ihrem Computer** kann KassenSturz nicht abwehren. Ein Programm, das als Sie läuft, kann Ihre Dateien lesen. Halten Sie Ihr System aktuell.
- **Snap-Paket unter Linux:** In manchen Ordnern erlaubt das System der Snap-Version nicht, das Buch gegen gleichzeitiges Öffnen zu sperren. KassenSturz sagt Ihnen das dann. Öffnen Sie dasselbe Buch in diesem Fall nicht in zwei Programmen gleichzeitig.
- **Windows** wird bisher vor allem automatisch getestet; Tests auf einem echten Windows-Computer laufen gerade an.
- **Übersetzungen** sind mit KI erstellt und werden nach und nach von Menschen geprüft. Ist ein Text unklar, sagen Sie es uns bitte.
- **Ein paar kleinere Funde sind noch in Arbeit**, zum Beispiel eine verständlichere Meldung bei voller Festplatte. Keiner davon gefährdet nach unserem Wissen Ihre Daten.

## Wenn Ihnen etwas auffällt

- **Sicherheitsproblem:** bitte nicht öffentlich melden, sondern vertraulich, wie in [SECURITY.md](../SECURITY.md) beschrieben.
- **Alles andere:** über die Testleitung oder als Eintrag („Issue“) auf GitHub.
- Schicken Sie nie echte Kontoauszüge, Kontonummern, PINs oder TANs mit, auch nicht als Bildschirmfoto.

---

*Für Entwickler und KI-Agenten: Wie wir angreifen und wo jeder Angriff festgehalten wird, steht in [SECURITY_LOG.md](SECURITY_LOG.md). Die technische Seite steht in [THREAT_MODEL.md](THREAT_MODEL.md).*
