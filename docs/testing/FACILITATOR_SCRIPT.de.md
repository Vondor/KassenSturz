# KassenSturz: Leitfaden für die Testleitung (Runde 1)

*Für die Person, die den Test begleitet; in jeder Runde ist das der Maintainer (D-552). Gehört zum [Fragebogen](QUESTIONNAIRE_1.de.md) und zum [Tester-Guide](TESTER_GUIDE.de.md). Dauer etwa 60 Minuten mit Vor- und Nachgespräch. Englische Fassung: [FACILITATOR_SCRIPT.en.md](FACILITATOR_SCRIPT.en.md).*

Die **eingerahmten Texte** liest die Testleitung **wörtlich vor**, damit jeder Tester dasselbe hört. Alles andere ist für die Testleitung selbst.

---

## 1. Vorbereitung (vor dem Termin)

- Die Installationsdatei liegt **nicht** bereit und nichts ist installiert: Karte 1 misst die Installation. Der Tester kennt nur die [Installationsblätter](README.md) und die Download-Seite.
- **Die Testleitung installiert nie für den Tester.** Auch nicht, wenn er es bittet.
- Die Beispielbücher kommen mit der App. Der passende Test-Kontoauszug für Karte 5 (aus dem Ordner `statements/`, siehe [README.md](README.md)) liegt auf dem Rechner des Testers bereit, ebenso die neuere Version für Karte 6.
- **Stimmen Beispielbuch und Kontoauszug zusammen?** Die Kontoauszüge und Lösungsblätter im Repository gehören zum Referenzmonat 2026-11 (siehe `reference-month.txt` im Ordner der Beispielbücher). Die App verschiebt die Daten eines Beispielbuchs beim Anlegen in den laufenden Monat und hält 24 Monate; ein Auszug aus 2026-11 passt dann nur zum Buch, wenn der Termin im November 2026 liegt. Vor jeder späteren Runde: `tools/example-books/generate.py --reference-month JJJJ-MM` für den Monat des Termins laufen lassen, den Test `example_books` die Lösungsblätter neu schreiben lassen und erst dann Auszüge und Lösungsblätter verwenden (Einzelheiten in der [README](README.md)). Die Aufgabe 4 und 5 am besten vorab einmal selbst durchspielen.
- KI und Bankverbindungen sind aus. Internet ist nach der Installation nicht nötig.
- Die Aufgabenkarten sind **einzeln** ausgedruckt oder als einzelne Nachrichten vorbereitet. Der Tester sieht immer nur die aktuelle Karte.
- Mitschrift bereit (Abschnitt 6), Uhr. Bei Videoanruf: Bildschirm geteilt, die Aufnahmefunktion bleibt **aus**.
- Ruhiger Ort, 60 Minuten ohne Störung, etwas zu trinken für den Tester.
- Die Absprache, auf welchem Weg der Tester seinen Fragebogen und seine Berichte schickt, steht (nur diese eine Person bekommt sie, siehe [Berichtsseite](REPORT_PAGE.de.md)).

## 2. Begrüßung (vorlesen)

> Vielen Dank, dass Sie sich Zeit nehmen. Ich heiße ____ und begleite heute den Test.
>
> Wir testen heute ein Programm für die Haushaltskasse, KassenSturz. Wichtig vorweg: **Wir testen das Programm, nicht Sie.** Sie können heute nichts falsch machen. Wenn etwas nicht klappt, liegt das am Programm, und genau das wollen wir finden. Ehrlich gesagt: Je mehr Sie stolpern und meckern, desto mehr hilft uns der Test.
>
> Ich habe an dem Programm mitgebaut. Gerade deshalb brauche ich Ihre ehrliche, auch harte Kritik. Sie tun mir damit einen Gefallen, keinen Schaden. Sagen Sie ruhig „Das ist umständlich“ oder „Das verstehe ich nicht“. Das sind für uns die wertvollsten Sätze.
>
> Wir arbeiten heute nur mit erfundenen Zahlen und den Beispielbüchern. Bitte geben Sie keine echten Konten, Beträge oder Banken ein. Wenn das Programm nach einem Kontostand fragt, denken Sie sich einen aus. Nichts davon verlässt Ihren Computer.
>
> Sie können jederzeit eine Pause machen oder aufhören, ohne einen Grund zu nennen.

## 2a. Darf ich aufnehmen? (vorlesen, bevor irgendetwas aufgenommen wird)

**Die Aufnahme ist noch aus.** Erst fragen, dann aufnehmen, nie umgekehrt. Auch bei Videoanrufen: die Aufnahmefunktion erst nach dem Ja einschalten.

> Bevor wir anfangen, eine Frage: Ich würde gern **Ton und Bildschirm aufnehmen**, damit ich mir später in Ruhe anhören kann, was Sie gesagt haben, und nicht alles mitschreiben muss.
>
> Die Aufnahme höre und sehe nur ich. Sie wird nicht veröffentlicht, nicht weitergegeben und nach der Auswertung gelöscht, spätestens nach ____ Wochen. Ihr Name steht nicht darauf.
>
> **Nein sagen ist völlig in Ordnung.** Der Test läuft dann genauso, ich schreibe nur mit. Sie können die Aufnahme auch jederzeit stoppen lassen, einfach „Stopp“ sagen.
>
> Darf ich die Aufnahme starten?

- **Nur bei einem klaren Ja** die Aufnahme einschalten. Zögern, „Na ja“ oder „Muss das sein?“ heißt **nein**.
- Nach dem Einschalten die Frage **auf der Aufnahme wiederholen**: „Die Aufnahme läuft jetzt. Sind Sie damit einverstanden?“, und das Ja abwarten. So ist die Zustimmung mit aufgezeichnet.
- Bei Nein: nicht nachfragen, nicht überreden. „Kein Problem, dann schreibe ich mit.“
- Sagt der Tester später „Stopp“: sofort beenden und fragen, ob der bisherige Teil gelöscht werden soll.
- Die Aufnahme enthält Stimme und Bildschirm und gilt wie ein Bericht mit persönlichen Daten: nur bei der Testleitung, nie in git, in einem Issue, in der Projektablage oder bei einem Agenten, nach der Auswertung gelöscht.
- In der Mitschrift ankreuzen: Aufnahme ☐ ja ☐ nein; wenn ja, Zustimmung auf der Aufnahme wiederholt ☐ ja.

## 3. Lautes Denken erklären und üben (vorlesen)

> Ich bitte Sie heute um etwas Ungewohntes: **laut denken.** Sagen Sie bitte alles, was Ihnen durch den Kopf geht, während Sie das Programm benutzen. Was Sie suchen, was Sie erwarten, was Sie wundert, was Sie ärgert. So, als würden Sie es einem Freund am Telefon erklären, der Ihren Bildschirm nicht sieht.
>
> Das fühlt sich am Anfang komisch an, das ist normal. Nach ein paar Minuten geht es von selbst. Meckern ist ausdrücklich erlaubt.
>
> Ich zeige es einmal kurz.

**Vormachen (etwa 30 Sekunden, mit einer Alltagsaufgabe, nicht mit KassenSturz):** zum Beispiel am Handy den Wecker auf 7 Uhr stellen und dabei laut denken: „Ich suche die Uhr-App … da unten ist ein Symbol, vielleicht das … nein, das ist der Timer … ah, hier ‚Wecker‘ … jetzt stelle ich 7 Uhr ein, hoffentlich ist das morgens und nicht abends …“

> Jetzt Sie, einmal zum Üben: Bitte sagen Sie mir laut, wie viele Fenster Ihre Wohnung hat, und zählen Sie dabei laut nach.

**Übung abwarten**, dann loben: „Genau so, perfekt.“ Nennt der Tester nur die Zahl: „Sehr gut, und jetzt bitte noch mal, aber sagen Sie, wie Sie im Kopf durch die Räume gehen.“

> Noch etwas: **Ich werde Ihnen während des Tests nicht helfen.** Das ist keine Unhöflichkeit. Ich möchte sehen, wie das Programm ohne Hilfe funktioniert, denn zu Hause sitzt auch niemand neben Ihnen. Das gilt auch für die Installation: Die erste Karte ist das Installieren, und das machen Sie allein mit dem Blatt. Ihre Fragen schreibe ich mir auf, und am Ende beantworte ich gern alles.
>
> Wenn Sie still werden, frage ich nur: „Was denken Sie gerade?“ Haben Sie dazu Fragen?

## 4. Während des Tests

**Ablauf:** zuerst die Fragen „Vor dem Test“ (Fragebogen, Abschnitt 1). Dann jede Aufgabenkarte **einzeln** geben:

> Hier ist die nächste Aufgabe. Lesen Sie sie bitte laut vor, und sagen Sie mir, wenn Sie meinen, Sie sind fertig.

Nach jeder Karte: „Wo haben Sie gestockt?“ und „Wie leicht war das, von 1 bis 5?“ Bei Karte 1 die Zeit stoppen.

**Was die Testleitung sagen darf** (neutral, ohne Richtung):
- „Was denken Sie gerade?“
- „Was suchen Sie?“
- „Was erwarten Sie, das jetzt passiert?“
- „Was hätten Sie hier erwartet?“
- „Erzählen Sie mehr.“ / „Mhm.“
- Auf eine Frage des Testers: „Was würden Sie denn vermuten?“ oder „Was würden Sie zu Hause jetzt tun?“

**Was die Testleitung nicht tut:**
- nicht helfen, nicht zeigen, nicht auf den Bildschirm deuten;
- keine Wörter aus dem Programm erklären („Teilbetrag heißt …“);
- nicht loben oder tadeln, wie der Tester etwas löst („Gut gemacht“, „Fast“). Erlaubt ist nur das Lob fürs laute Denken: „Danke, das hilft sehr.“;
- keine Suggestivfragen („War das schwer?“, besser: „Wie war das?“);
- nicht seufzen, nicht lachen, nicht nicken, wenn der Tester richtig liegt.

**Zögern notieren:** Jedes Zögern, jedes „Hm“, jedes Zurückgehen, jedes Suchen mit Uhrzeit (oder Zählerstand der Aufnahme) und Karte in die Mitschrift. Zögern ist ein Fund, auch wenn die Karte am Ende geschafft wird.

**Wenn der Tester festhängt:** nach etwa 3 Minuten ohne Fortschritt oder bei sichtbarem Frust:
> Das ist eine sehr wichtige Stelle für uns, danke. Möchten Sie es noch kurz weiter versuchen, oder gehen wir zur nächsten Karte?

Erst danach, falls die nächste Karte darauf aufbaut, darf die Testleitung den Weg zeigen. In der Mitschrift notieren: „Hilfe gegeben bei …“. **Bei Karte 1 (Installation):** dem Tester nur sagen, bei welchem Schritt des Blatts er ist, nie, wohin er klicken soll; steckt er fest, die Stelle notieren und weitergehen lassen (zum Beispiel mit dem Hinweis auf den nächsten Absatz des Blatts).

**Wenn der Tester echte Daten eintippt:** nicht unterbrechen oder rügen. Mitschrift: „Echte Daten: ja“. Am Ende Frage 8 einfach stellen.

**Wenn das Programm abstürzt:** Uhrzeit notieren, ruhig bleiben:
> Das ist ein Fehler im Programm, sehr gut, dass wir ihn gefunden haben. Wir starten es einfach neu.

## 5. Abschluss (vorlesen)

> Das waren alle Aufgaben, vielen Dank. Jetzt habe ich noch ein paar Fragen, danach dürfen Sie mich alles fragen.

Dann die Fragen „Nach dem Test“ (Fragebogen, Abschnitt 3), Frage 8 zuletzt und freundlich:
> Eine letzte Frage, und jede Antwort ist hier in Ordnung: Haben Sie heute irgendwo echte eigene Daten eingegeben? Ich frage nur, damit wir Ihre Notizen entsprechend schützen.

Danach Fragen des Testers beantworten und zeigen, was er gesucht hat. Dann erklären, wohin Fragebogen und Berichte gehen ([Berichtsseite](REPORT_PAGE.de.md)). Zum Schluss:
> Sie haben uns heute sehr geholfen. Alles, was Sie gesagt haben, fließt direkt in die nächste Version ein.

Nach dem Termin: KassenSturz ohne Entwicklermodus neu starten lassen (oder deinstallieren). Mitschrift ohne Namen ablegen: nur ein Kürzel.

## 5a. Wann ist die App gut? Erfolgskriterien (nur für die Testleitung, nicht vorlesen)

Hier steht, **woran die Testleitung erkennt, ob eine Karte geschafft ist**. Pro Karte gibt es drei Ergebnisse:

- **Geschafft:** ohne Hilfe, ohne falsches Ergebnis, innerhalb der Zielzeit.
- **Mit Mühe:** ohne Hilfe geschafft, aber über der Zielzeit, mit Umwegen oder einem Fehler, den der Tester selbst bemerkt und behoben hat.
- **Nicht geschafft:** aufgegeben, Hilfe gebraucht, oder ein falsches Ergebnis, das der Tester für richtig hält. **Das Letzte ist das Schlimmste**, denn dann vertraut jemand falschen Zahlen.

| Karte | Geschafft, wenn … | Zielzeit | Prüfen nach der Karte |
|---|---|---|---|
| 1 Installieren und starten | die Übersicht mit einem selbst angelegten Konto erscheint, „Entwicklermodus“ im Fenstertitel, und das allein nach dem Installationsblatt | 10 Min. | Steht „Entwicklermodus“ oben? Stimmt der Kontostand mit dem ausgedachten überein? An welchem Schritt des Blatts wurde gestockt? |
| 2 Alltagsausgaben | alle drei Buchungen mit richtigem Betrag, Datum und Richtung (Ausgabe/Einnahme) im Konto stehen; Supermarkt und Kaffee in einer passenden Kategorie | 5 Min. | Kontostand = Anfangsstand − 42,50 − 3,20 + 1.800 |
| 3 Korrigieren und umbuchen | Kaffee steht auf 2,30; 200 € sind als Umbuchung zwischen zwei eigenen Konten gebucht (nicht als Ausgabe); der letzte Schritt ist rückgängig gemacht | 7 Min. | Zählt die Übersicht die Umbuchung **nicht** als Geld rein/raus? Weiß der Tester, welcher Schritt rückgängig gemacht wurde? |
| 4 Wo ist mein Geld hin? | die genannte Summe für Lebensmittel und die Antwort „mehr rein/mehr raus“ stimmen mit dem Lösungsblatt des Beispielbuchs überein | 3 Min. | Antwort mit dem Lösungsblatt vergleichen |
| 5 Kontoauszug einlesen | die Datei ist im richtigen Konto, die doppelte Buchung wurde **nicht** zweimal übernommen, der Tester hat die Vorschau gelesen, bevor er bestätigt | 7 Min. | Anzahl neuer Buchungen = Lösungsblatt; Kontostand = Endsaldo der Datei (siehe Hinweis unten) |
| 6 Aktualisieren | neue Version installiert, das Buch öffnet ohne Fehlermeldung, die Buchungen aus Karte 2 sind da | 5 Min. | Versionsnummer unter „Über“ ist die neue |
| 7 Sprache und Wörter | (keine Lösung; jede Fundstelle zählt) | 3 Min. | Liste der Wörter übernehmen |
| 8 Tiefer einsteigen (freiwillig) | eine Regel sortiert Kaffee-Käufe von selbst; ein neues Diagramm ist auf der Übersicht | 7 Min. | Neue Kaffee-Buchung eintragen: landet sie in der Kategorie? |

**Lösungsblätter (nur für die Testleitung, nie dem Tester zeigen):** Zu jedem Beispielbuch gibt es ein Lösungsblatt, das das Team aus dem echten Import- und Berichtscode erzeugt, nie von Hand (Schritt M7a.8): für Karte 4 die Lebensmittel-Summe des Vormonats und ob mehr rein- oder rausging, für Karte 5 die Anzahl der Zeilen, der neuen und der doppelten Buchungen und den Endsaldo. Die Pfade stehen in [README.md](README.md); das Lösungsblatt für Karte 4 und 5 ist je Buch eine Datei.

**Hinweis zu Karte 5 mit der Vorstadtfamilie:** Liegt der Tag, an dem der Tester testet, vor dem 6. November 2026, zeigt das Konto „Girokonto“ nach dem Einlesen der CAMT-Datei „Darin Buchungen nach heute: −201,15 €“. Das ist gewollt: Ein Teil der Zeilen der Datei trägt ein Datum, das für den Rechner des Testers noch in der Zukunft liegt. Sagen Sie dem Tester dazu nichts vorab; fragt er danach, gilt die neutrale Rückfrage („Was würden Sie vermuten?“). Der Schlusssaldo laut Datei bleibt trotzdem der Vergleichswert. Ab dem 6. November 2026 erscheint der Hinweis nicht mehr.

### Wann ist die Runde bestanden?

Die App ist **bereit für die nächste Runde**, wenn über alle Tester einer Runde:

1. **Keine einzige** Karte mit einem falschen Ergebnis endet, das der Tester für richtig hält (Karten 2 bis 5). Ein solcher Fall ist ein Fehler der höchsten Stufe, egal wie selten.
2. Die Karten 1 bis 6 von **mindestens 4 von 5 Testern geschafft** werden (Geschafft oder Mit Mühe), und keine dieser Karten von mehr als einem Tester nur „Mit Mühe“.
3. Die **Leichtigkeit im Schnitt mindestens 4 von 5** ist und keine Karte im Schnitt unter 3 liegt.
4. Bei Frage 3.1 („Vertrauen Sie den Zahlen?“) **niemand „nein“** sagt.
5. Bei Frage 3.6 („weiter benutzen?“) **mindestens die Hälfte „ja“ oder „vielleicht“** sagt.
6. **Kein Absturz und kein Datenverlust** (Karte 6: alles noch da).

Jede Stelle, an der **zwei oder mehr Tester** stocken, wird ein Fund für die nächste Version, auch wenn die Karte am Ende geschafft wurde.

*Bei Runde 1 mit nur einem Tester (dem Maintainer) gelten 1, 4 und 6 voll; 2, 3 und 5 werden erst mit mehreren Testern gezählt.*

## 6. Mitschrift der Testleitung

| Zeit | Karte | Was der Tester sagte oder tat (wörtlich, wenn möglich) | Gestockt? | Hilfe gegeben? | Ergebnis (geschafft / mit Mühe / nicht) |
|---|---|---|---|---|---|
| | | | | | |

Am Ende ergänzen: Datum, Testleitung, Tester-Kürzel (kein Name), Computer, Versionsnummer, Aufnahme ja/nein, echte Daten ja/nein. **Mit echten Daten:** Mitschrift und Aufnahme nur bei der Testleitung, nie in git, in der öffentlichen Fehlerliste oder bei einem Agenten, nach der Auswertung löschen.

Die drei auffälligsten Stellen des Tests, sofort danach notiert:
1. ____
2. ____
3. ____
