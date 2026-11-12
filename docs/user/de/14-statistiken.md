# 14. Statistiken und Diagramme

*Mit dem Statistik-Baukasten bauen Sie Diagramme, Tabellen und Zahlen zu Ihrem Geld, speichern sie unter einem Namen und legen sie auf eine Seite. Alle Zahlen kommen aus Ihren Buchungen und sind auf den Cent genau.*

## Eine neue Auswertung bauen

Öffnen Sie den **Statistik-Baukasten** (**Schritt für Schritt führen**). Eine **Neue Auswertung** führt in drei Schritten, links steht die Liste der Schritte, unten **Zurück**, **Weiter**, **Fertig** und **Abbrechen**. Die **Vorschau** steht in jedem Schritt unter den Fragen, Sie sehen also sofort, was Ihre Wahl bewirkt.

1. **Zeitraum:** Zur Auswahl stehen **Dieser Monat**, **Letzter Monat**, **Die letzten 12 Monate**, **Dieses Jahr**, **Letztes Jahr** und ein eigener Zeitraum von bis (siehe unten).
2. **Kategorien wählen:** Sie sagen, welche Kategorien zählen, einzeln oder als Gruppe (Seite 6).
3. **Aussehen und Vorschau:** die Art, der Titel, die Legende und der Name.

Unter **Erweitert: Formeln und Feineinstellungen** und **Erweitert: aus Listen wählen** liegt, was Sie meist nicht brauchen. Leere Felder zeigen blasse Beispiele und nie sofort Fehler. **Alle Einstellungen auf einmal zeigen** öffnet alles auf einer Seite.

Verlassen Sie den Baukasten vor **Fertig**, geht nichts verloren, solange KassenSturz läuft: Die Auswertung ist beim nächsten Öffnen am selben Schritt wieder da. **Abbrechen** beginnt neu.

## Die Arten von Diagrammen

| Art | Zeigt |
|---|---|
| **Balken**, **Gestapelte Balken** | Beträge je Kategorie oder je Monat; gestapelt sehen Sie die Teile eines Ganzen |
| **Torte**, **Ring** | Anteile an einer Gesamtsumme |
| **Linie**, **Gefüllte Linie** | wie sich ein Betrag über die Zeit ändert |
| **Tabelle** | die Zahlen selbst, mit Summe |
| **Eine Zahl** | eine einzelne Zahl, auch aus einer Formel (Seite 15), mit Vergleich zum Zeitraum davor |
| **Fortschrittsbalken** | wie weit Sie auf einem Ziel sind, aus einer Formel |

Nur das Bild ändert sich, wenn Sie die Art wechseln; die Zahlen und Summen bleiben gleich.

**Kontostand als Kerzen:** Auf einem Dashboard finden Sie unter **Anordnung bearbeiten › Zur Seite hinzufügen › Diagramme** auch **Kontostand als Kerzen**. Es zeigt den Kontostand der gewählten Konten (ohne Auswahl: alle Giro-, Spar- und Bargeldkonten zusammen) als eine Kerze je Tag, Woche oder Monat. In den **Eigenschaften** wählen Sie unter **Eine Kerze je** *Tag*, *Woche (Montag bis Sonntag)* oder *Monat*; *Automatisch* richtet sich nach dem Zeitraum: ganze Monate, etwa die letzten 3 Monate, bekommen eine Kerze je Monat; sonst bis 31 Tage eine Kerze je Tag, bis ein halbes Jahr eine je Woche und darüber eine je Monat. Angeboten werden nur die Längen, die mindestens zwei Kerzen ergeben, für die letzten 7 Tage also weder *Woche* noch *Monat*; eine früher gewählte Länge, die nicht mehr zum Zeitraum passt, wird wie *Automatisch* gezeichnet und gilt wieder, sobald der Zeitraum lang genug ist. Der Kasten reicht vom Stand am Anfang bis zum Stand am Ende, hohl, wenn der Stand gestiegen ist, gefüllt, wenn er gesunken ist; unter **Kerzenkörper** lässt sich das umdrehen, oder alle Kästen werden gefüllt oder hohl gezeichnet, und die Legende zeigt die Kästen so, wie sie gezeichnet sind (dann unterscheiden sich steigende und fallende Kerzen nur durch die Farbe); die dünne Linie reicht vom niedrigsten bis zum höchsten Stand am Ende eines Tages. Mehrere Buchungen an einem Tag zählen nur mit dem Stand am Ende des Tages. Jede Währung bekommt ihre eigenen Kerzen.

**Eigene Diagramme:** Ein Dashboard kann auch ein **Eigenes Diagramm** halten, das nach einer kurzen Beschreibung in der Sprache Vega-Lite gezeichnet wird. In seinen **Eigenschaften** setzt **Beispiel einfügen** eine fertige Beschreibung in das Feld, zum Beispiel den Kontostand als Balken mit Ein- und Ausgängen als Linien; Sie können sie dort ändern, gespeichert wird sie erst mit **Diese Beschreibung verwenden**. Welche Datennamen und Felder es gibt, zeigt die Hilfeseite **Hilfe: Muster, Formeln und Diagramme** unter **Einstellungen**.

**Richtung der Balken:** In den **Eigenschaften** eines Balkendiagramms steht **Balken** auf *Automatisch* (Zeilen nur, wenn die Namen zu lang für die Balken sind), *Quer (eine Zeile je Name)* oder *Aufrecht (Namen darunter)*.

## Zeiträume, die mit dem heutigen Tag wandern

Überall, wo nach einem Zeitraum gefragt wird, gibt es nach den Schnellwahlen **Die letzten Monate …** und **Ein Gehaltsmonat …**:

- **Die letzten Monate:** Sie wählen, wie viele. **Nur vollständige Monate (diesen Monat weglassen)** ist zuerst angehakt, weil ein Monat, der noch nicht um ist, die Zahlen klein wirken lässt.
- **Ein Gehaltsmonat:** läuft von einem Tag Ihrer Wahl, etwa dem Zahltag, bis zum Tag davor im nächsten Monat. Fehlt der Tag in einem kurzen Monat (der 31. im April), gilt der letzte Tag. Sie wählen **Den laufenden** oder **Den davor**.
- **Ein Zeitraum für die ganze Seite:** Wählen Sie ihn in der Filterleiste, zählen alle Diagramme, Tabellen, Zahlenkarten und Formeln, die der Seite folgen, dieselben Tage.

## Ausgaben als Durchschnitt

In den **Eigenschaften** von Torte, Ring, Balken oder Tabelle für Ausgaben steht **Anzeigen als**: **Summe** oder **Durchschnitt je Monat**, **je Quartal**, **je Jahr**. Geteilt wird durch die Monate des Zeitraums bis heute: „Die letzten 3 Monate“ mit vollständigen Monaten wird durch 3 geteilt; dieses Jahr im November durch 10, weil der Dezember noch nicht war. Jeder Betrag und die Summe werden einmal aus den genauen Beträgen geteilt.

## Geld rein und raus mit eigenen Listen

Ein Diagramm von Geld rein und raus kann als **Balken** nebeneinander gezeichnet werden. In den **Eigenschaften** zählen **Geld rein: nur diese Kategorien**, **Geld rein: nur diese Konten**, **Geld raus: nur diese Kategorien** und **Geld raus: nur diese Konten** nur, was Sie ankreuzen, für diese Seite allein. Nichts angekreuzt heißt: Diese Seite zählt wie das übrige Diagramm. Geld raus eines Kontos enthält auch noch nicht Zugeordnetes und lässt Umbuchungen zwischen eigenen Konten weg.

## Speichern, wiederfinden, ändern

- Unter **Gespeicherte Ansichten** stehen Ihre Auswertungen. **Im Assistenten ändern:** (Zeitraum …, Kategorien …, Aussehen und Titel …) öffnet sie wieder in den Schritten; **Fertig** behält die Änderung.
- **Umbenennen**, **Löschen** und **Als neue Ansicht speichern** gibt es dort auch. Alles lässt sich mit **Rückgängig** zurücknehmen.
- **Fertig ohne Namen** behält eine neue Auswertung als „Auswertung 1“, „Auswertung 2“ und so weiter.
- **Auf die Übersicht legen** (unter einer geöffneten Ansicht) setzt sie als eigenes Diagramm auf die Übersicht. Eine spätere Änderung der gespeicherten Ansicht erreicht dieses Diagramm nicht.
- Ein Diagramm auf einer Seite ändern Sie über **Anordnung bearbeiten**, **Eigenschaften**, **Im Statistik-Baukasten Schritt für Schritt ändern**.

## Farben

Alle Diagramme teilen eine Palette: **Einstellungen › Diagrammfarben**. **Palette bearbeiten …** (im letzten Schritt des Baukastens und in den Eigenschaften eines Diagramms) führt dorthin. KassenSturz warnt, wenn eine Farbe vor dem Hintergrund schwer zu erkennen ist.

**Wenn etwas anders aussieht:** „Für diesen Zeitraum gibt es noch nichts anzuzeigen“ heißt, dass in den gewählten Tagen keine passenden Buchungen liegen. Wählen Sie einen anderen Zeitraum oder tragen Sie Buchungen ein.
