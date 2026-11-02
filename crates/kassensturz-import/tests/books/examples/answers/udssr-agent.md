<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Antwortbogen: UdSSR Government Agent (Hauptverwaltung für Windstille)

Dieses Blatt schreibt der Test `example_books` (`crates/kassensturz-pipeline/tests/example_books.rs`) aus dem echten Import- und Berichtscode, nicht von Hand. Das Buch und die Kontoauszüge macht `tools/example-books/generate.py`; nichts darin gehört zu einer echten Person.

Referenzmonat: 2026-11 (der letzte Monat ist Oktober 2026). Haben Sie das Buch zu einem späteren Zeitpunkt aufgemacht, zeigt die App den dann letzten Monat; die Zahlen unten gelten für Oktober 2026.

## Karte 4: Übersicht

- Ausgaben für „Verpflegung“ im Oktober 2026: **135,26 RUB**
- Eingenommen im Oktober 2026: 753,10 RUB; ausgegeben: 219,47 RUB; Unterschied: 533,63 RUB
- Ist mehr hereingekommen als hinausgegangen? **Ja**

## Karte 5: Kontoauszug einlesen (Konto „Gehaltskonto“)

Beide Dateien liegen in `crates/kassensturz-import/tests/books/examples/statements/` und enthalten dieselben 9 Zeilen; genau eine davon steht schon im Buch.

| Datei | Zeilen | neu | schon im Buch (doppelt) | sieht gleich aus (Sie entscheiden) | Schlusssaldo laut Bank | Kontostand nach dem Einlesen |
|---|---|---|---|---|---|---|
| `udssr-agent.camt053.xml` | 9 | 8 | 1 | 0 | 1.201,56 RUB | 1.201,56 RUB |
| `udssr-agent.csv` | 9 | 8 | 0 | 1 | (eine CSV-Datei hat keinen) | – |

Eine CAMT-Datei trägt für jede Zeile die Nummer der Bank; deshalb erkennt KassenSturz die vorhandene Zeile sicher als doppelt. Eine CSV-Datei hat keine solche Nummer; die vorhandene Zeile hat denselben Betrag und fast dasselbe Datum wie eine Buchung im Buch und wird deshalb nur als „sieht gleich aus“ gezeigt, und Sie entscheiden, ob sie trotzdem eingelesen wird. Die übrigen 8 Zeilen sind neu. Liest man zuerst die CAMT-Datei und danach die CSV-Datei, sieht KassenSturz alle Zeilen schon im Buch.

Die CSV-Einstellungen (Trennzeichen „;“, Dezimalkomma, Tag.Monat.Jahr, Spalten Buchungstag, Valuta, Name Zahlungsbeteiligter, IBAN Zahlungsbeteiligter, Verwendungszweck, Betrag, Währung) stehen in `udssr-agent.csv.settings`.
