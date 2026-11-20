# 22. Was als Nächstes kommt

*Eine Übersicht in einfachen Worten: was KassenSturz schon kann, woran wir gerade arbeiten und was erst nach Version 1.0 geplant ist. Das ist ein Plan, kein Versprechen: Reihenfolge und Inhalt können sich ändern, wenn Tester etwas Wichtigeres finden.*

## Wo wir stehen

KassenSturz ist in der **Beta**. Das heißt: Alle geplanten Funktionen für Version 1.0 sind da oder in Arbeit, und uns ist kein Fehler bekannt, bei dem Daten verloren gehen. Eingeladene Tester probieren die App mit ausgedachten Büchern aus, und ihre Funde bestimmen, was als Nächstes gebaut wird.

Es gilt: **Geprüft und sicher zu benutzen: Viele Sicherheitsprüfungen und Angriffstests sind gemacht; die App kann sich aber noch ändern.**

## Schon da

- Konten, Kategorien, Buchungen, Umbuchungen, Rückgängig und Verlauf
- Kontoauszüge einlesen und direkte Bankverbindung, nur zum Lesen
- Regeln, die Buchungen von selbst sortieren
- Budgets, geplante Zahlungen und Statistiken mit Dashboards
- KI-Hilfe, die aus ist, bis Sie sie einschalten

Wie das alles funktioniert, steht in den anderen Seiten dieses [Handbuchs](README.md).

## Woran wir gerade arbeiten

Der aktuelle Abschnitt ist der letzte der Beta. Er soll alles erledigen, was vor Version 1.0 noch fehlt:

- **Ein Willkommens-Assistent** für neue Bücher: Bank oder Datei einlesen, den Kontostand bestätigen, die ersten Buchungen sortieren. Er ist freiwillig, jeder Schritt lässt sich überspringen.
- **Bank und Kategorien einfacher:** Der PIN-Dialog sagt klarer, welche PIN gemeint ist. Nach einer erfolgreichen Verbindung fragt die App, ob sie die Buchungen gleich holen soll. In jeder Kategorieauswahl gibt es **Neue Kategorie …**.
- **Dashboards:** Kacheln lassen sich an jedem Rand in der Größe ändern, Tags gibt es in der Filterleiste, und geplante Zahlungen lassen sich **pausieren** und später **fortsetzen**, ohne den Verlauf zu verlieren.
- **Plugins:** Eigene Erweiterungen für Importer, Exporter, Berichte, Automatisierungen, Designs und KI-Anbieter. Jedes Plugin zeigt vorher, was es darf.
- **Kleinere Korrekturen** aus den letzten Testrunden, zum Beispiel für Rechnungsordner unter Windows und die Beschriftung von „Rückgängig“.

## Danach: Version 1.0

Vor der ersten stabilen Version prüfen wir noch einmal gründlich den Code, der mit Geld, Passwörtern, Bankverbindungen und dem Aufbau der Dateien zu tun hat. Auch die Texte und Bilder für die Stores werden fertig. Ab dem stabilen Stand ändert sich das Dateiformat des Buchs nicht mehr, außer mit einer automatischen Sicherung und einer sauberen Übernahme der alten Bücher.

Version 1.0 erklärt allein der Entwickler.

## Nach Version 1.0

Das ist geplant, aber noch nicht begonnen:

- Wertpapiere und Depot, später Kryptowährungen
- Kredite, geplant aus den Zahlungen, die wirklich stattgefunden haben
- Ein Importer für GnuCash-Bücher
- Sicherungen in Ihrer eigenen Cloud und optionale Verschlüsselung des Buchs
- Eine Begleit-App für Android zum Fotografieren von Belegen
- Ein Posteingang für Belege, den Ihre eigene KI lesen kann

## Was KassenSturz nicht werden soll

Keine Firmenbuchhaltung, keine Steuererklärung, keine Anlageberatung, kein Dienst, der Ihre Daten in einer Cloud des Projekts speichert, keine Werbung und kein Zahlungsversand. Ihre Daten bleiben auf Ihrem Rechner.

## Mitreden

Neuigkeiten und Pläne stehen in den [Ankündigungen](https://github.com/Vondor/KassenSturz/discussions/categories/announcements), Wünsche und Ideen sammeln wir in den [Ideen](https://github.com/Vondor/KassenSturz/discussions/categories/ideas). Dort können Sie kommentieren und mit einem Daumen zeigen, was Ihnen am wichtigsten ist. Fehler melden Sie bitte weiter als Issue (siehe [Wenn etwas nicht klappt](19-probleme.md)).

Bitte schreiben Sie dort keine echten Kontodaten, Namen oder Kontonummern.

## Wenn etwas anders aussieht

Dieser Plan ändert sich häufiger als der Rest des Handbuchs. Den genauen Stand je Schritt finden Sie in der [Roadmap des Projekts](../../ROADMAP.md) (Englisch).
