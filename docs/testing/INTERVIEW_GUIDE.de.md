# KassenSturz: Leitfaden für die Testleitung (Beta)

*Für die Person, die den Termin begleitet. Gehört zum [Tester-Guide](TESTER_GUIDE.de.md) und zum [Installationsprotokoll](INSTALL_LOG.de.md). Dauer: 30 bis 60 Minuten. Die Testleitung passt ihn an.*

Die **eingerahmten Texte** liest die Testleitung **wörtlich vor**, damit jede Testperson dasselbe hört. Alles andere ist für die Testleitung selbst.

---

## 0. Worum es in diesem Test geht (nur für die Testleitung)

Wir suchen **keine Fehler**. Wir wollen verstehen, **wie KassenSturz auf Menschen wirkt**: was sie zuerst sehen, was sie sich darunter vorstellen, wo sie sich sicher fühlen und wo nicht, und welche Wege sie von selbst gehen. Die Testperson wird durch einige Funktionen geführt, und wir hören zu, wie sie sie erlebt.

Daraus folgt für den ganzen Termin:

- **Nicht jede Funktion ist für jeden Menschen gedacht.** Wer nur wissen will, wohin das Geld geht, braucht keine Formeln. Wer täglich mit Tabellen rechnet, sucht vielleicht genau die. Wer programmiert, will Dinge selbst bauen. Darum gibt es in Teil C **drei Wege**, und die Testperson geht nur einen davon.
- **Ein Abbruch ist kein Misserfolg**, sondern eine Auskunft: „Das ist nichts für mich“ ist ein Befund über Passung, nicht über Fähigkeit.
- **Abstürze und Fehler** werden notiert, aber nicht gesucht und nicht vertieft. Wenn etwas kaputtgeht, geht es ruhig weiter.
- Es zählt, **was die Testperson sagt und erwartet**, nicht nur, ob sie ans Ziel kommt.

## 1. Vorbereitung (vor dem Termin)

- **Installation ist schon erledigt.** Die Testperson hat zu Hause allein installiert, nur mit dem Installationsblatt, und das Installationsprotokoll geschickt. Die Testleitung liest es vorher. Steckte die Testperson fest, gab es genau einen Hinweis per E-Mail; er ist im Protokoll notiert. **Die Testleitung installiert nie für die Testperson.**
- **Beispielbuch:** Im Termin arbeitet die Testperson **nur im Beispielbuch Vorstadtfamilie**. Sie legt kein eigenes Buch an. Das Beispielbuch ist vor Beginn geöffnet, auf der Übersicht.
- KassenSturz läuft im **Entwicklermodus** (steht oben im Fenster). Bankverbindungen sind aus. **KI ist zu Beginn aus**; sie wird nur auf Weg 4 eingeschaltet, und zwar von der Testperson selbst.
- **Für Weg 4 (nur lange Fassung):** Die Testleitung hält eine **Zugangskarte** bereit: Adresse des Modells und ein **eigener, befristeter Schlüssel nur für diese Testperson**, der nach dem Termin gesperrt wird. Auf der Karte steht die **vollständige Adresse mit `http://` und Port**, so wie sie eingetippt werden muss. Alle Testpersonen bekommen **dasselbe Modell**. Vorher einmal prüfen, dass das Modell erreichbar ist.
- **Aufnahme von Bildschirm und Ton vorbereiten, aber noch nicht starten** (Abschnitt 3). **So wird aufgenommen:** Die Testperson arbeitet an ihrem eigenen Rechner und teilt im **Videoanruf** ihren **ganzen Bildschirm** (nicht nur ein Fenster, sonst fehlen Dialoge). Ein Anruf im Browser genügt, damit auf ihrem Rechner nichts installiert werden muss. **Aufgenommen wird nur auf dem Rechner der Testleitung** (Bildschirmfreigabe und beide Stimmen), nie auf dem Rechner der Testperson; so bleibt dort keine Aufnahme zurück, und das Löschen nach 4 Wochen liegt in einer Hand. Das gilt auch, wenn die Testleitung im selben Raum sitzt. Sitzen beide im selben Raum, bekommt **jede Person ein eigenes Mikrofon auf einer eigenen Tonspur** (in OBS zwei Audiospuren), damit sich die Stimmen später trennen lassen. Vorher einmal mit einer Probeaufnahme prüfen, dass Bild und beide Stimmen ankommen und Text auf dem Bildschirm lesbar ist. Gleich nach dem Start der Aufnahme die Uhrzeit laut sagen („Es ist jetzt 18 Uhr 32“); damit lassen sich Aufnahme und Protokoll des Entwicklermodus später auf die Sekunde zusammenlegen.
- **Das Protokoll des Entwicklermodus** bleibt auf dem Rechner der Testperson. Sie schickt es nach dem Termin mit ihren Notizen an die Testleitung (siehe [Berichtsseite](REPORT_PAGE.de.md)).
- **Bildschirm aufräumen:** Die Aufnahme zeigt alles, was auf dem Bildschirm erscheint. Bitten Sie die Testperson vorher, andere Programme, E-Mail, Chat und Benachrichtigungen zu schließen oder stummzuschalten, und keine eigenen Dateien oder Ordner offen zu haben.
- **Tastatur prüfen:** Arbeitet die Testperson an einem Testrechner oder in einer virtuellen Maschine, ist die Tastatur auf **Deutsch** gestellt. Sonst sucht sie Zeichen wie den Doppelpunkt, und das misst den Testaufbau statt des Programms.
- Die Karten liegen **einzeln** bereit (ausgedruckt oder als einzelne Nachrichten). Die Testperson sieht immer nur die aktuelle Karte. Von den Karten in Teil C nur die des gewählten Wegs.
- Mitschrift (Abschnitt 8) und Uhr bereit. Ruhiger Ort, 60 Minuten ohne Störung, etwas zu trinken.
- Geklärt: auf welchem Weg die Testperson Notizen oder Nachträge schickt (nur an die Testleitung, siehe [Berichtsseite](REPORT_PAGE.de.md)).

### Zeitplan

| Teil | Was | Kurz (30 Min.) | Lang (60 Min.) |
|---|---|---|---|
| 2 | Begrüßung | 2 | 3 |
| 3 | Aufnahme: Einwilligung | 2 | 2 |
| 4 | Lautes Denken üben, drei Vorfragen | 3 | 5 |
| 5 | Teil A: Der erste Blick | 3 | 5 |
| 6 | Teil B: Im Alltag | 6 | 12 |
| 6 | Teil C: Ein Weg nach Wahl | 8 | 20 |
| 7 | Nachgespräch | 6 | 13 |

Für die kurze Fassung: in Teil B nur Karte B1 und B2, in Teil C nur die erste Karte des Wegs, im Nachgespräch nur die Fragen mit ★.

## 2. Begrüßung (vorlesen)

> Vielen Dank, dass Sie sich Zeit nehmen. Ich heiße ____ und begleite heute das Gespräch.
>
> KassenSturz ist ein Programm für die Haushaltskasse, und es ist noch in Arbeit. Heute möchte ich vor allem eines wissen: **Wie wirkt das Programm auf Sie?** Was Sie denken, wenn Sie es sehen, was Sie erwarten, was Ihnen gefällt und was nicht. Sie müssen keine Fehler suchen, und Sie können nichts falsch machen. Es gibt keine richtigen oder falschen Antworten.
>
> Ich habe an dem Programm mitgearbeitet. Bitte seien Sie trotzdem ganz offen, auch wenn Ihnen etwas nicht gefällt. Das hilft mir mehr als Höflichkeit.
>
> Wir arbeiten heute in einem **Beispielbuch** mit einer ausgedachten Familie und ausgedachten Zahlen. **Bitte geben Sie keine echten eigenen Daten ein**, also keine eigenen Konten, Beträge, Banken oder IBANs.
>
> Sie können jederzeit eine Pause machen oder aufhören, ohne einen Grund zu nennen.

## 3. Darf ich aufnehmen? (vorlesen, bevor irgendetwas aufgenommen wird)

**Die Aufnahme ist noch aus.** Erst fragen, dann aufnehmen, nie umgekehrt.

> Ich würde gern **Ihren Bildschirm und den Ton aufnehmen**, also das, was Sie sagen, und was Sie auf dem Bildschirm tun. Dann kann ich Ihnen jetzt zuhören, statt mitzuschreiben, und mir später alles in Ruhe ansehen.
>
> Die Aufnahme sehe und höre nur ich. Sie wird nicht veröffentlicht, nicht weitergegeben und spätestens nach 4 Wochen gelöscht. Was ich daraus aufschreibe, enthält Ihren Namen nicht.
>
> **Nein sagen ist völlig in Ordnung.** Das Gespräch läuft dann genauso, ich schreibe nur mit. Sie können die Aufnahme auch jederzeit anhalten lassen, sagen Sie einfach „Stopp“.
>
> Darf ich die Aufnahme starten?

- **Nur bei einem klaren Ja** einschalten. Zögern, „Na ja“ oder „Muss das sein?“ heißt **nein**.
- Nach dem Einschalten auf der Aufnahme wiederholen: „Die Aufnahme läuft jetzt. Sind Sie damit einverstanden?“ und das Ja abwarten.
- Bei Nein: nicht nachfragen, nicht überreden. „Kein Problem, dann schreibe ich mit.“
- Sagt die Testperson später „Stopp“: sofort anhalten und fragen, ob der bisherige Teil gelöscht werden soll.
- Die Aufnahme gilt als persönliche Daten: nur bei der Testleitung, nie in git, in einem Issue, in der Projektablage oder bei einem Agenten; spätestens nach 4 Wochen gelöscht. Bevor ein Agent etwas daraus bekommt, werden Namen, Stimmen und alles, was auf eine Person, eine Bank oder ein Programm schließen lässt, entfernt.
- Mitschrift: Aufnahme ☐ ja ☐ nein; Zustimmung auf der Aufnahme wiederholt ☐ ja.

## 4. Lautes Denken üben und drei Vorfragen

> Ich bitte Sie heute um etwas Ungewohntes: **laut denken.** Sagen Sie bitte alles, was Ihnen durch den Kopf geht: was Sie sehen, was Sie suchen, was Sie erwarten, was Sie wundert. So, als würden Sie es jemandem am Telefon erzählen, der Ihren Bildschirm nicht sieht. Das fühlt sich am Anfang komisch an; nach ein paar Minuten geht es von selbst.
>
> Ich zeige es einmal kurz.

**Vormachen (etwa 30 Sekunden, nicht mit KassenSturz):** am Handy einen Wecker auf 7 Uhr stellen und dabei laut denken („Ich suche die Uhr … das ist der Timer, nein … hier ‚Wecker‘ … hoffentlich ist das morgens …“).

> Jetzt Sie, zum Üben: Sagen Sie mir laut, wie viele Fenster Ihre Wohnung hat, und gehen Sie dabei in Gedanken durch die Räume.

Danach: „Genau so, danke.“

> Noch etwas: **Ich werde Ihnen während der Aufgaben nicht helfen** und nichts erklären. Das ist keine Unhöflichkeit; ich möchte sehen, wie das Programm ohne Hilfe auf Sie wirkt. Ihre Fragen schreibe ich auf, und am Ende beantworte ich alles gern. Wenn Sie still werden, frage ich nur: „Was denken Sie gerade?“

**Drei Vorfragen** (mündlich, Antwort in die Mitschrift):

1. Wie behalten Sie heute im Blick, wofür Ihr Geld ausgegeben wird? Erzählen Sie einfach kurz.
2. Arbeiten Sie sonst mit Tabellen, etwa Excel oder LibreOffice Calc? ☐ kaum ☐ manchmal ☐ oft, auch mit Formeln
3. Programmieren Sie, beruflich oder in der Freizeit? ☐ nein ☐ ein wenig ☐ ja

Die Antworten auf 2 und 3 helfen später beim Vorschlag für Teil C. Sie werden **nicht** vorgelesen oder kommentiert.

## 5. Teil A: Der erste Blick

Die Übersicht der Vorstadtfamilie ist offen. Die Testperson hat noch nichts angeklickt.

> Das ist KassenSturz mit dem Beispielbuch einer ausgedachten Familie. Bitte klicken Sie noch nichts an. Schauen Sie sich die Seite in Ruhe an und erzählen Sie mir, was Sie sehen.

Danach, eine nach der anderen, mit Pausen:

> Wofür, glauben Sie, ist dieses Programm gedacht? Und für wen?
>
> Wie geht es der Familie mit ihrem Geld, so wie Sie das hier sehen?
>
> Wenn Sie jetzt etwas anklicken dürften: was zuerst, und was erwarten Sie dann?

**Beobachten:** Welche Teile der Seite werden genannt, welche übersehen? Welche Wörter greift die Testperson auf, welche umschreibt sie? Wie spricht sie über die Familie (bewertend, beschreibend, sich vergleichend)?

## 6. Die Karten

Jede Karte **einzeln** geben:

> Hier ist die nächste Karte. Lesen Sie sie bitte laut vor, und sagen Sie mir, wenn Sie meinen, Sie sind fertig, oder wenn Sie nicht weitermachen möchten.

**Nach jeder Karte** genau diese zwei Fragen, immer gleich:

> Wie war das für Sie?
>
> Was haben Sie erwartet, und was ist dann passiert?

Keine Punkteskala nach jeder Karte. Wertungen holt das Nachgespräch ein.

### Teil B: Im Alltag (alle Testpersonen)

**Karte B1: Wohin ging das Geld?**
> Die Familie fragt sich, wofür im letzten Monat das meiste Geld ausgegeben wurde. Finden Sie es heraus.

**Karte B2: Ein Einkauf**
> Gestern hat jemand aus der Familie für 23,80 € im Supermarkt eingekauft. Tragen Sie das ein.

**Karte B3: Wieder zurück** (nur lange Fassung)
> Sie haben es sich anders überlegt: Der Einkauf soll doch nicht im Buch stehen. Sorgen Sie dafür.

**Karte B4: Kommt das wieder?** (nur lange Fassung)
> Die Familie zahlt jeden Monat Miete. Sorgen Sie dafür, dass das Programm damit rechnet, auch für die nächsten Monate.

### Teil C: Ein Weg nach Wahl

Die Testleitung liest die drei Wege vor (oder legt die Karte „Drei Wege“ hin) und **lässt die Testperson wählen**:

> Jetzt geht es um Auswertungen, also darum, Zahlen und Diagramme zum Geld der Familie zu bekommen. Dafür gibt es drei Wege, in der langen Fassung vier. Bitte wählen Sie den, der Ihnen am meisten liegt. Es gibt keinen besseren oder schlechteren Weg.
>
> **Weg 1: Zusammenklicken.** Sie lassen sich Schritt für Schritt führen und wählen aus, was Sie sehen möchten.
>
> **Weg 2: Rechnen wie in einer Tabelle.** Sie schreiben selbst, was berechnet werden soll, ähnlich wie Formeln in Excel.
>
> **Weg 3: Selbst bauen.** Sie beschreiben ein Diagramm oder eine Suchregel in einer kleinen Sprache, ähnlich wie beim Programmieren.
>
> **Weg 4: Fragen stellen.** Sie schalten eine KI ein und fragen sie in Ihren eigenen Worten. *(nur lange Fassung)*

- **Nicht vorschlagen, bevor die Testperson gewählt hat.** Fragt sie „Was soll ich nehmen?“, antworten Sie: „Was würden Sie zu Hause nehmen?“
- In der Mitschrift notieren: gewählter Weg, Begründung in ihren Worten, und ob die Wahl zu den Vorfragen 2 und 3 passt. Eine Wahl, die nicht zu den Vorfragen passt, ist **besonders aufschlussreich**; nicht umlenken.
- **Ein zweiter Weg (nur lange Fassung, wenn Zeit bleibt):** „Möchten Sie noch einen der anderen Wege ausprobieren?“ Nur anbieten, nie drängen, nie einen bestimmten vorschlagen. Die Testperson wählt wieder selbst; jede Kombination ist möglich, auch Weg 4 als zweiter. Höchstens zwei Wege, damit jeder genug Zeit bekommt (etwa 10 Minuten je Weg). Die **Reihenfolge** notieren: Wer zuerst selbst eine Antwort gefunden hat und dann die KI fragt, vergleicht anders als jemand, der mit der KI beginnt. Das ist gewollt und wird im Nachgespräch aufgegriffen.

**Weg 1, Karte C1-1: Ein Bild vom Geld**
> Bauen Sie ein Diagramm, das zeigt, wofür die Familie in den letzten drei Monaten ihr Geld ausgegeben hat. Wenn es Ihnen gefällt, legen Sie es auf die Startseite.

**Weg 1, Karte C1-2: Vergleichen** (nur lange Fassung)
> Die Familie möchte wissen, ob sie dieses Jahr für Lebensmittel mehr ausgibt als letztes Jahr. Zeigen Sie es ihr so, wie Sie es selbst gern sehen würden.

**Weg 2, Karte C2-1: Ein Anteil**
> Die Familie möchte auf ihrer Startseite eine Zahl sehen: Wie viel Prozent ihrer Ausgaben im letzten Monat waren Lebensmittel? Richten Sie das ein.

**Weg 2, Karte C2-2: Was übrig bleibt** (nur lange Fassung)
> Richten Sie eine Zahl ein, die zeigt, wie viel vom Gehalt übrig bleibt, wenn die Miete bezahlt ist. Sie soll immer für den Monat gelten, den die Seite gerade zeigt.

**Weg 3, Karte C3-1: Ein eigenes Diagramm**
> Legen Sie auf einer Seite ein Diagramm an, dessen Aussehen Sie selbst beschreiben. Ändern Sie daran etwas, das Sie selbst interessiert, zum Beispiel Farbe, Art oder was gezeigt wird.

**Weg 3, Karte C3-2: Eine Suchregel** (nur lange Fassung)
> Das Programm soll Buchungen von selbst einer Kategorie zuordnen, wenn ihr Text einer bestimmten Form folgt, etwa jede Buchung, in deren Text eine Rechnungsnummer steht. Richten Sie so etwas ein.

**Weg 4, Karte C4-1: Die KI einschalten**
> Sie möchten dem Programm Fragen in Ihren eigenen Worten stellen. Dafür gibt es eine KI, die zuerst eingeschaltet werden muss. Auf dieser Karte stehen die Adresse und Ihr Zugang. Richten Sie das ein.

Dazu die Zugangskarte geben. Die Testleitung richtet **nicht** selbst ein und tippt nichts ein. Die Zustimmung, die das Programm dabei abfragt, gibt die Testperson selbst; sie ist so mit auf der Aufnahme. Hängt die Testperson länger als 5 Minuten, gilt die Regel „Wenn die Testperson festhängt“; die Zeit fürs Einrichten wird getrennt von C4-2 notiert.

**Bei den Zustimmungsseiten schweigt die Testleitung**, auch wenn die Testperson fragt („Muss ich da zustimmen?“): dann nur „Was würden Sie zu Hause tun?“. Stimmt die Testperson nicht zu, ist das ein gültiges Ergebnis; es geht ohne KI weiter (Mitschrift: „KI abgelehnt“).

**Findet die Testperson den Chat nach dem Einschalten nicht:** Nach etwa 3 Minuten sagt die Testleitung nur, **wo** er ist, und gibt **keine Frage vor**. Mitschrift mit Uhrzeit: „Chat gezeigt“.

**Weg 4, Karte C4-2: Fragen**
> Fragen Sie die KI, wofür die Familie im letzten Monat am meisten ausgegeben hat. Fragen Sie danach etwas, das Sie selbst über das Geld der Familie wissen möchten.

**Beobachten bei Weg 4:** Wie wird die Zustimmung gelesen (gelesen, überflogen, weggeklickt)? Wird die Antwort der KI mit den Zahlen auf der Seite verglichen? Wie geht die Testperson mit einem Vorschlag um, den sie erst bestätigen muss? Wird die KI wie ein Werkzeug oder wie ein Gegenüber angesprochen?

**Nach Weg 4:** Die Testperson schaltet die KI in den Einstellungen wieder aus. Die Testleitung sperrt den Schlüssel nach dem Termin.

**Zum Schluss jedes Wegs** (vorlesen):
> Wenn Sie sich hier etwas wünschen dürften, das es noch nicht gibt: Was würden Sie bauen oder sehen wollen?

### Was die Testleitung sagen darf

- „Was denken Sie gerade?“ / „Was suchen Sie?“ / „Was erwarten Sie, das jetzt passiert?“
- „Erzählen Sie mehr.“ / „Mhm.“
- Auf eine Frage der Testperson: „Was würden Sie vermuten?“ oder „Was würden Sie zu Hause jetzt tun?“

### Was die Testleitung nicht tut

- nicht helfen, nicht zeigen, nicht auf den Bildschirm deuten, keine Wörter aus dem Programm erklären;
- nicht bewerten, wie die Testperson etwas löst („Gut“, „Fast“); erlaubt ist nur: „Danke, das hilft sehr.“;
- keine Suggestivfragen („War das schwer?“, „Gefällt Ihnen das?“; besser: „Wie war das?“);
- keine Namen von Knöpfen oder Seiten nennen, die die Testperson noch nicht selbst genannt hat;
- nicht seufzen, nicht lachen, nicht nicken.

### Wenn die Testperson festhängt

Nach etwa 3 Minuten ohne Fortschritt oder bei sichtbarem Frust:
> Danke, das ist eine aufschlussreiche Stelle. Möchten Sie es noch kurz weiter versuchen, oder gehen wir zur nächsten Karte?

Erst danach darf die Testleitung den Weg zeigen, wenn die nächste Karte darauf aufbaut. Mitschrift: „Hilfe gegeben bei …“.

**Liegt das Problem an unserem Aufbau, nicht am Programm** (falsche Tastatur, Fehler auf einer Karte, Videoanruf, Rechner der Testleitung), darf die Testleitung es sofort technisch lösen. Mitschrift mit Uhrzeit: „Technische Hilfe: …“. Diese Zeit zählt nicht als Zeit am Programm.

**Wenn die Testperson echte Daten eintippt:** nicht unterbrechen oder rügen. Mitschrift: „Echte Daten: ja“. Am Ende nachfragen (Frage 9).

**Wenn das Programm abstürzt:** Uhrzeit notieren.
> Das liegt am Programm, nicht an Ihnen. Wir starten es neu und machen weiter.

## 7. Nachgespräch (vorlesen)

> Das waren die Karten, vielen Dank. Jetzt möchte ich Sie noch ein paar Dinge fragen. Danach dürfen Sie mich alles fragen.

Die Fragen offen stellen, in dieser Reihenfolge, und Zeit lassen. Nachfragen nur mit „Erzählen Sie mehr“ oder „Woran machen Sie das fest?“. Fragen mit ★ gehören auch in die kurze Fassung.

1. ★ Wenn Sie jemandem in einem Satz erzählen müssten, was KassenSturz ist: Was würden Sie sagen?
2. ★ Wie hat das Programm auf Sie gewirkt? Gibt es ein Wort, das es beschreibt?
3. Gab es einen Moment, in dem Sie sich sicher gefühlt haben, und einen, in dem Sie unsicher waren? Erzählen Sie von beiden. Und wie haben Sie sich selbst heute beim Bedienen erlebt? Ist das bei anderen Programmen ähnlich oder anders?
4. ★ Die Zahlen heute waren ausgedacht. Wenn es Ihre eigenen wären: Wie sehr würden Sie solchen Zahlen trauen? Woran machen Sie das fest?
5. Gab es ein Wort oder einen Text, der Ihnen fremd vorkam oder bei dem Sie etwas anderes erwartet hätten?
6. ★ Teil C (den gewählten Weg aus der Mitschrift nennen, z. B. „Sie haben Weg 1 gewählt“): Warum haben Sie diesen Weg gewählt? Hätten Sie, im Nachhinein, lieber einen anderen genommen? (Bei zwei Wegen: Wie war der zweite im Vergleich zum ersten?)
7. Für wen, glauben Sie, ist dieses Programm gemacht? Gehören Sie dazu?
8. ★ Wie halten Sie Ihre eigenen Geldsachen auf dem Computer heute vor anderen geschützt, und was würden Sie dabei von so einem Programm erwarten? Was müsste es geben, damit Sie ihm Ihre Geldsachen anvertrauen?
9. Eine letzte Frage, und jede Antwort ist in Ordnung: Haben Sie heute irgendwo echte eigene Daten eingegeben? Ich frage nur, damit wir Ihre Aufnahme und Notizen entsprechend schützen.

Danach Fragen der Testperson beantworten und zeigen, was sie gesucht hat. Zum Schluss:
> Sie haben uns heute sehr geholfen. Vielen Dank.

**Nach dem Termin:** Aufnahme anhalten und sicher ablegen (nur bei der Testleitung). KassenSturz ohne Entwicklermodus neu starten lassen oder deinstallieren. Mitschrift nur mit Kürzel (T1, T2, …) ablegen.

## 8. Mitschrift der Testleitung

Kopf: Datum · Kürzel (kein Name) · Computer · Versionsnummer · Fassung kurz/lang · Aufnahme ja/nein · echte Daten ja/nein · bei Weg 4: Modell, Einrichtungszeit, Schlüssel gesperrt ☐

Vorfragen: 1 ____ · 2 ☐ kaum ☐ manchmal ☐ oft · 3 ☐ nein ☐ ein wenig ☐ ja
Teil C: erster Weg ☐ 1 ☐ 2 ☐ 3 ☐ 4 · zweiter Weg ☐ keiner ☐ 1 ☐ 2 ☐ 3 ☐ 4 · Begründung (wörtlich) ____ · passt zu den Vorfragen ☐ ja ☐ nein

| Zeit (Aufnahme) | Teil / Karte | Was die Testperson sagte oder tat (wörtlich, wenn möglich) | Erwartung ↔ was geschah | Hilfe gegeben? |
|---|---|---|---|---|
| | | | | |

**Beobachten statt bewerten:** Notieren Sie, was gesagt und getan wurde, nicht, ob es „gut“ war. Besonders: Stellen, an denen Sprechen und Tun auseinandergehen (sagt „ach, einfach“ und sucht dann lange), und Stellen, an denen die Testperson sich mit anderen vergleicht („das ist wohl eher was für …“).

Die drei auffälligsten Momente, sofort danach notiert:
1. ____
2. ____
3. ____

Mit echten Daten: Mitschrift und Aufnahme nur bei der Testleitung, nie in git, in der Fehlerliste oder bei einem Agenten; nach der Auswertung löschen.
