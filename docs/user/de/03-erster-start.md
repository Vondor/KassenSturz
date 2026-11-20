# 3. Der erste Start

*Diese Seite führt durch den Einrichtungsassistenten. Er fragt nur das Nötigste, und es wird erst etwas gespeichert, wenn Sie einen Schritt abschließen.*

Beim allerersten Start öffnet sich der Assistent. Sie können in jedem Schritt zurückgehen. Schließen Sie das Fenster mittendrin, beginnt der Assistent beim nächsten Mal von vorn; nichts ist dann angelegt.

## Schritt 1: Willkommen

Sie wählen Ihre **Sprache** (die Ihres Systems ist schon eingestellt) und die **Währung**, die Sie verwenden. Meist bestätigen Sie nur mit **Weiter**. Ändern Sie die Sprache, startet KassenSturz einmal neu in dieser Sprache; es geht nichts verloren.

## Schritt 2: Wie möchten Sie anfangen?

![Auswahl, wie Sie anfangen möchten](../../testing/screenshots/windows/12-start-choice.png)

Vier Wege:

- **Geführt einrichten:** Schritt für Schritt mit Ihren eigenen Daten; jeden Schritt können Sie überspringen.
- **Mit meinen eigenen Konten:** etwa zwei Minuten.
- **Ein Beispielbuch ausprobieren:** ein ausgedachter Haushalt zum Umsehen.
- **Meine Daten aus einem anderen Programm holen:** ein KMyMoney-Buch oder eine zuvor mit KassenSturz gespeicherte Kopie.

Wer schon ein KassenSturz-Buch hat, nimmt den kleinen Link **Ein KassenSturz-Haushaltsbuch öffnen, das ich schon habe…**.

## Der Weg „Geführt einrichten“

Die geführte Einrichtung führt Sie mit Ihrem eigenen Haushaltsbuch durch das Wichtigste. Jede Seite hat **Überspringen**, und **Einrichtung beenden** bringt Sie sofort zur Übersicht. Nichts wird für Sie erledigt: Jeder Schritt öffnet die normale Seite, und Sie speichern dort, was Sie möchten.

1. **Welche Konten haben Sie?** Die Art und ein Name. Wie viel darauf ist, tippen Sie nicht ein: Das kommt von Ihrer Bank oder aus einer Datei.
2. **Buchungen holen:** mit Ihrer Bank verbinden oder eine Datei von ihr importieren. Sie sehen zuerst eine Vorschau.
3. **Stimmt dieser Kontostand?** Vergleichen Sie mit Ihrer Banking-App. Weicht er ab, tippen Sie ein, was Ihre Bank zeigt, und KassenSturz bietet an, den Anfangsstand zu korrigieren.
4. **Buchungen zuordnen:** Wählen Sie für einen Namen eine Kategorie, und alle Buchungen mit diesem Namen werden auf einmal zugeordnet. Danach bietet KassenSturz eine Regel an, die das künftig von selbst macht (Seite 12).
5. **Ihr erstes Diagramm** mit dem Statistik-Baukasten (Seite 14).
6. **Ein Budget und eine geplante Zahlung** (Seiten 13 und 21).
7. **Ihre Startseite gestalten:** Kacheln verschieben, in der Größe ändern, hinzufügen und entfernen (Seite 4).
8. **Haben Sie eine KI auf diesem Computer?** Nur eine Frage. „Nein, oder ich weiß es nicht“ ist schon gewählt, und dann bleiben die KI-Funktionen ausgeschaltet und nichts über KI wird angezeigt. „Ja“ schaltet sie ein und öffnet die Seite, auf der Sie die Adresse Ihrer KI eintragen (Seite 18). Versionen von KassenSturz, die ohne KI gebaut sind, haben diesen Schritt nicht.

Jeden Schritt finden Sie später wieder, auch die übersprungenen: **Einstellungen → Hilfe: Geführte Einrichtung**. Die letzte Seite verlinkt außerdem dieses Handbuch im Wiki.

## Der Weg „Mit meinen eigenen Konten“

1. Legen Sie Ihre Konten an: die **Art** (Girokonto, Sparkonto, Bargeld, Kreditkarte), einen **Namen** und **wie viel heute darin ist**. Nehmen Sie den Betrag aus Ihrer Banking-App oder vom Kontoauszug, damit KassenSturz von dem ausgeht, was wirklich da ist. Leer gelassen heißt: nichts drin.
2. Mit **Haushaltsbuch anlegen** wird alles auf einmal gespeichert.
3. KassenSturz fragt, wie neue Buchungen hineinkommen: **Ich tippe sie selbst ein**, **Eine Datei von meiner Bank importieren** oder **Mit meiner Bank verbinden**. Das lässt sich später ändern.
4. Sie sehen die **Übersicht** mit einer kurzen Meldung, dass Ihr Haushaltsbuch bereit ist.

KassenSturz legt für Sie **Kategorien** an (Einnahmen wie Gehalt, Ausgaben wie Wohnen, Essen, Gesundheit). Sie gehören Ihnen: umbenennen, verschieben und ergänzen dürfen Sie alles (Seite 6).

## Der Weg „Ein Beispielbuch ausprobieren“

![Die vier Beispielbücher](../../testing/screenshots/windows/13-example-books.png)

Zur Auswahl stehen:

- *Vorstadtfamilie*: ein ganz normaler Haushalt, der empfohlene Einstieg.
- *Blumen-Utopia*: eine kleine Gartengenossenschaft mit gemeinsamer Kasse, Budgets und Diagrammen.
- *Weltraumkolonie*: fünf Jahre in einer Marskolonie.
- *UdSSR-Agent*: ein ausgedachter Agent der 1980er Jahre mit mehreren Währungen.

Es entsteht eine **eigene Kopie**; das mitgelieferte Beispiel bleibt unverändert. Sie können darin nach Herzenslust ändern und löschen. Später öffnen Sie ein Beispielbuch erneut über **Einstellungen › Anderes Haushaltsbuch öffnen…**.

## Der Weg „Daten aus einem anderen Programm“

KassenSturz liest Bücher von **KMyMoney** (Dateien auf `.kmy` oder SQLite) und eine früher mit KassenSturz gespeicherte Kopie. Gespeicherte Berichte aus KMyMoney kommen als Ansichten in den Statistiken mit (Seite 16). Eine Datei wird nur gelesen, nie verändert.

## Wo liegt mein Haushaltsbuch?

KassenSturz legt es selbst in einem eigenen Ordner an und öffnet es bei jedem Start wieder; Sie müssen keine Datei auswählen. Unter **Einstellungen** öffnen Sie ein anderes Buch (**Anderes Haushaltsbuch öffnen…**) oder legen ein neues an (**Neues Haushaltsbuch anlegen…**), zum Beispiel zum Üben.

Ein Buch ist immer nur an **einem** Ort geöffnet. Liegt es auf einem Netzlaufwerk, warnt KassenSturz. Meldet es „Dieses Buch ist auf … geöffnet“, schließen Sie es dort und wählen **Erneut versuchen**.

**Wenn etwas anders aussieht:** Hat Ihre Version zusätzliche Schritte, lesen Sie die Frage auf dem Bildschirm; jeder Schritt sagt, was als Nächstes passiert.
