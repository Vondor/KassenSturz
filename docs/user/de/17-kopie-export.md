# 17. Kopie speichern und exportieren

*Ihre Daten gehören Ihnen. Diese Seite zeigt, wie Sie sie als Tabelle oder als ganze Sicherung mitnehmen und ein Dashboard weitergeben.*

## Eine Kopie meiner Daten speichern

**Einstellungen › Ihre Daten › Eine Kopie meiner Daten speichern…** bietet zwei Wege:

- **Für eine Tabelle:** die Buchungen der gewählten Konten als Datei, die jede Tabellenkalkulation öffnet, zum Beispiel LibreOffice Calc, eine Zeile je Buchung und Konto.
  Unter den Konten können Sie einen **ersten** und einen **letzten Tag** angeben; beide Tage gehören dazu, ein leeres Feld heißt alle Tage. Die Zeilen bleiben genau, wie sie sind: Sie ergeben den Kontostand eines Kontos nur, wenn nichts vor dem ersten Tag weggelassen wird. In der Kommandozeile: `export DATEI --from 2026-01-01 --until 2026-12-31`.
- **Das ganze Haushaltsbuch speichern…:** alles in einer Datei (Konten, Kategorien, Buchungen, Regeln, Wechselkurse, Verknüpfungen mit Bankkonten), aus der KassenSturz ein neues Buch anlegen kann, zum Beispiel als Sicherung oder auf einem anderen Computer. Der Verlauf kommt nicht mit.

KassenSturz **überschreibt dabei nie** eine vorhandene Datei. In der Datei stehen **keine** Bank-Anmeldenamen und keine PIN.

## Ordner, die KassenSturz öffnen darf

Nur in der Flatpak-Version. Wenn Sie einen Ordner wählen (für Ihr Haushaltsbuch oder für Sicherungskopien), darf KassenSturz ihn öffnen, bis Sie das zurücknehmen: **Einstellungen › Ihre Daten › Ordner, die KassenSturz öffnen darf** listet sie auf, mit **Diesen Ordner vergessen** bei jedem. Vergessen beendet nur den Zugriff; **es wird keine Datei gelöscht, verschoben oder geändert**. Ein Ordner, den Ihr Haushaltsbuch oder Ihre Sicherungskopien noch nutzen, sagt das und kann vergessen werden, sobald Sie beides woanders ablegen.

## Ein Buch aus einer Datei anlegen

Beim ersten Start (**Meine Daten aus einem anderen Programm holen**, Seite 3) oder unter **Einstellungen › Neues Haushaltsbuch anlegen…**.

## Sicherungen von KassenSturz selbst

Vor dem Anpassen eines Buches an eine neue Version legt KassenSturz selbst eine Sicherung an (**Einstellungen › Sicherungen**) und behält die letzten zehn. Eine eigene Kopie von Zeit zu Zeit bleibt trotzdem klug.

## Dashboards weitergeben

**Dieses Dashboard in einer Datei speichern** schreibt ein Dashboard in eine kleine Datei (`.json`); **Dashboard aus einer Datei hinzufügen** liest sie in ein anderes Buch. Die Datei enthält **kein Geld**: keine Beträge, Buchungen, Kontostände oder Kontonummern, nur die Namen der Konten und Kategorien, auf die das Dashboard zeigt (Seite 4).

## Ihre Daten verlassen den Computer nie von allein

Ohne Ihr ausdrückliches Ja wird nichts verschickt. KassenSturz sendet keine Nutzungsdaten, Fehlerberichte oder Update-Anfragen.

**Wenn etwas anders aussieht:** Meldet KassenSturz, dass es eine Datei nicht lesen kann, steht in der Meldung, was zu tun ist. Eine Datei aus einer sehr alten Entwicklungsversion lässt sich unter Umständen nicht mehr einlesen.
