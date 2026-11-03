# Antwortbogen: Suburban Family (Familie Sommerfeld)

Dieses Blatt schreibt der Test `example_books` (`crates/kassensturz-pipeline/tests/example_books.rs`) aus dem echten Import- und Berichtscode, nicht von Hand. Das Buch und die Kontoauszüge macht `tools/example-books/generate.py`; nichts darin gehört zu einer echten Person.

Referenzmonat: 2026-11 (der letzte Monat ist Oktober 2026). Das Buch reicht etwa zwei Jahre zurück. Legen Sie es in einem späteren Monat an, verschiebt KassenSturz alle Daten um ganze Monate, sodass der Referenzmonat der laufende Monat ist; die Zahlen unten gelten dann für den Monat vor dem laufenden. Buchungen nach dem heutigen Tag lässt KassenSturz weg. Die Kontoauszüge passen nur im Referenzmonat selbst zum Buch.

## Karte 4: Übersicht

- Ausgaben für „Essen“ im Oktober 2026: **795,43 EUR**
- Eingenommen im Oktober 2026: 4.374,55 EUR; ausgegeben: 3.412,23 EUR; Unterschied: 962,32 EUR
- Ist mehr hereingekommen als hinausgegangen? **Ja**

## Karte 5: Kontoauszug einlesen (Konto „Girokonto“)

Beide Dateien liegen in `crates/kassensturz-import/tests/books/examples/statements/` und enthalten dieselben 9 Zeilen; genau eine davon steht schon im Buch.

| Datei | Zeilen | neu | schon im Buch (doppelt) | sieht gleich aus (Sie entscheiden) | Schlusssaldo laut Bank | Kontostand nach dem Einlesen |
|---|---|---|---|---|---|---|
| `suburban-family.camt053.xml` | 9 | 8 | 1 | 0 | 17.925,47 EUR | 17.925,47 EUR |
| `suburban-family.csv` | 9 | 8 | 0 | 1 | (eine CSV-Datei hat keinen) | – |

Eine CAMT-Datei trägt für jede Zeile die Nummer der Bank; deshalb erkennt KassenSturz die vorhandene Zeile sicher als doppelt. Eine CSV-Datei hat keine solche Nummer; die vorhandene Zeile hat denselben Betrag und fast dasselbe Datum wie eine Buchung im Buch und wird deshalb nur als „sieht gleich aus“ gezeigt, und Sie entscheiden, ob sie trotzdem eingelesen wird. Die übrigen 8 Zeilen sind neu. Liest man zuerst die CAMT-Datei und danach die CSV-Datei, sieht KassenSturz alle Zeilen schon im Buch.

Die CSV-Einstellungen (Trennzeichen „;“, Dezimalkomma, Tag.Monat.Jahr, Spalten Buchungstag, Valuta, Name Zahlungsbeteiligter, IBAN Zahlungsbeteiligter, Verwendungszweck, Betrag, Währung) stehen in `suburban-family.csv.settings`.
