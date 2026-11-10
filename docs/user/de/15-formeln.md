# 15. Formeln und Muster

*Eine Formel rechnet mit Ihren Kategorien wie eine Tabellenkalkulation. Diese Seite zeigt, was möglich ist, an ausgedachten Beispielen. Muster brauchen nur Regeln, und nur wer es genauer will.*

## Wo Sie Formeln schreiben

In den **Eigenschaften** einer **Zahl** oder eines **Fortschrittsbalkens** (und im Statistik-Baukasten unter **Erweitert**). Neben der Formel steht das Ergebnis, sodass Sie ihr nicht blind vertrauen müssen. Fehler sagen in Worten, was nicht stimmt („Reisen ist weder eine Kategorie noch eine Gruppe“).

## Namen

Ein Name steht für eine Kategorie oder eine Ihrer Gruppen und ergibt deren Summe im Zeitraum der Zahl, als positiver Betrag (eine Erstattung macht sie kleiner). Alles darin zählt mit.

- Ein einfacher Name: `Lebensmittel`.
- Mit Leerzeichen oder Sonderzeichen in einfachen Anführungszeichen: `'Essen gehen'`.
- Ein Pfad für eine Kategorie in einer anderen: `'Alltag > Haustiere'`.
- Gibt es eine Einnahmen- und eine Ausgabenkategorie desselben Namens, setzen Sie `Income.` oder `Spending.` davor: `Spending.Haustiere`.
- **Alles ausgegeben, alles eingenommen:** `Spending.ALL` und `Income.ALL`, auch das noch nicht Zugeordnete.

Konten sind keine Namen. Alle Namen einer Formel müssen **dieselbe Währung** haben.

## Rechnen

`+`, `-`, `*`, `/`, Klammern und `%`. Beispiel: Was vom Gehalt nach der Miete bleibt:

`=Gehalt - Miete`

Ein Betrag durch einen Betrag ist eine Zahl (ein Verhältnis); zwei Beträge malzunehmen verweigert KassenSturz. Alles wird mit genauen Dezimalzahlen gerechnet; **nur die Division rundet** (auf 10 Stellen), und das Ergebnis wird einmal am Ende gerundet. Teilen durch null ergibt **keinen Wert**, nie null.

## Der Zeitraum eines Namens

Sie schreiben `@` und einen Zeitraum hinter den Namen:

- Wörter in Anführungszeichen: `"this month"`, `"last month"`, `"this quarter"`, `"last quarter"`, `"this year"`, `"last year"`, `"last 12 months"`, `"last 3 months"`, `"last 3 complete months"` (die Zahl von 1 bis 120). Die Zeitraumwörter schreiben Sie **englisch**, wie die Funktionsnamen.
- Daten: `@2026` (ein Jahr), `@2026-03` (ein Monat), `@2026-01..2026-03` (Monate von bis), `@2026-01-15..2026-02-14` (Tage von bis).

Beispiele:

- Durchschnitt der letzten drei vollen Monate: `=Lebensmittel@"last 3 complete months" / 3`
- Miete der letzten sechs Monate: `=Miete@"last 6 months"`

Die Zeiträume zählen vom Zeitraum der Zahl, nicht von heute: Zeigt die Seite den März, ist `"last month"` der Februar.

## Funktionen

Die Namen der Funktionen sind englisch und gleich in jeder Sprache, im Editor erklärt eine Liste jede in einem Satz.

| Funktion | Gibt |
|---|---|
| `SUM(a; b; …)` | die Summe |
| `AVERAGE(a; b; …)` | den Durchschnitt |
| `MIN(…)`, `MAX(…)` | den kleinsten, den größten Wert |
| `COUNT(Name; …)` | wie viele Buchungen im Zeitraum zu den Kategorien gehören (jede einmal) |
| `ABS(x)`, `SIGN(x)` | ohne Vorzeichen; −1, 0 oder 1 |
| `ROUND(x; Stellen)`, `ROUNDUP`, `ROUNDDOWN` | gerundet (kaufmännisch, von null weg, zu null hin) |
| `INT(x)` | abgerundet auf eine ganze Zahl |
| `IF(Test; ja; nein)` | je nach Test das eine oder das andere |
| `AND`, `OR`, `NOT` | Wahrheitswerte für `IF` |
| `IFERROR(x; Ersatz)` | `Ersatz`, wenn `x` keinen Wert hat |
| `PERCENT_OF(Teil; Ganzes)` | den Anteil in Prozent |
| `CHANGE(neu; alt)` | die Änderung in Prozent |

Als Trennzeichen zwischen den Teilen gilt bei deutscher Zahlendarstellung `;` und das Dezimalkomma (`=ROUND(Haustiere * 1,5; 2)`). `;` wird in jeder Darstellung angenommen, auch wenn Sie eine Formel aus einer anderen Anleitung kopieren.

Beispiele:

- Wie viel Prozent der Ausgaben sind Lebensmittel? `=PERCENT_OF(Lebensmittel; Spending.ALL)`
- Hat sich Reisen verändert? `=CHANGE(Reisen; Reisen@"last year")`
- Nur ein Überschuss, nie ein Minus: `=IF(Gehalt - Miete > 0; Gehalt - Miete; 0)`

## Formeln und eigene Listen

Die Listen „Geld rein: nur diese …“ gelten für Formeln **nicht**: Ein Name in einer Formel zählt die Konten und Kategorien des ganzen Diagramms. Die Hilfe in den Eigenschaften sagt das auch.

## Muster in Regeln

In einer Bedingung einer Regel (Seite 12) können Sie **Muster (fortgeschritten)** wählen. Sie finden dann Texte, die einer Form entsprechen, statt nur ein Wort. Das ist nie nötig, und die Hilfe in der App erklärt, wie Sie es schreiben, mit ausgedachten Beispielen.

**Wenn etwas anders aussieht:** Die Funktionen auf dieser Seite sind die der aktuellen Version. Kommt eine Meldung mit „unbekannte Funktion“, steht sie hier nicht und gibt es noch nicht.
