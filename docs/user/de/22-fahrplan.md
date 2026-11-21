# 22. Was als Nächstes kommt

*Eine Übersicht in einfachen Worten: was KassenSturz schon kann, was bis Version 1.0 noch kommt und was erst danach geplant ist. Das ist ein Plan, kein Versprechen: Reihenfolge und Inhalt können sich ändern, wenn Tester etwas Wichtigeres finden.*

## Wo wir stehen

Seit dem 21. November 2026 gibt es eine **stabile Ausgabe** von KassenSturz. Sie ist für den Alltag mit Ihrem eigenen Haushaltsbuch gedacht. Daneben gibt es **Beta-Ausgaben**: Sie sind neuer, können mehr Fehler haben und sind zum Ausprobieren da (Seite 2).

KassenSturz als Ganzes ist weiter in der **Beta**-Phase, bis Version 1.0 erklärt wird; so lange beginnt jede Versionsnummer mit `0.`. Uns ist kein Fehler bekannt, bei dem Daten verloren gehen. Tester probieren die App weiter mit ausgedachten Büchern aus, und ihre Funde bestimmen, was als Nächstes gebaut wird.

Vor der stabilen Ausgabe wurde der Code, der mit Geld, Passwörtern, Bankverbindungen und dem Aufbau der Dateien zu tun hat, noch einmal gründlich geprüft. Seitdem ändert sich das Dateiformat des Buchs nicht mehr, außer mit einer automatischen Sicherung und einer sauberen Übernahme der alten Bücher.

## Schon da

- Konten, Kategorien, Buchungen, Umbuchungen, Rückgängig und Verlauf
- Kontoauszüge einlesen; unter Linux auch die direkte Bankverbindung, nur zum Lesen
- Regeln, die Buchungen von selbst sortieren
- Budgets, geplante Zahlungen (auch zum Pausieren) und Statistiken mit Dashboards
- Schlagwörter, auch als Filter auf den Dashboards
- Eine geführte Einrichtung für neue Bücher; sie ist freiwillig, und jeder Schritt lässt sich überspringen
- Plugins: eigene Erweiterungen für Importer, Exporter, Berichte, Automatisierungen, Designs und KI-Anbieter. Jedes Plugin zeigt vorher, was es darf.
- KI-Hilfe, die aus ist, bis Sie sie einschalten

Wie das alles funktioniert, steht in den anderen Seiten dieses [Handbuchs](README.md).

## Bis Version 1.0

- Fehler beheben, die Tester und Nutzer finden
- Die Bankverbindung für Windows ist vorgesehen, hat aber noch keinen Termin; bis dahin lesen Sie dort Dateien ein
- Eine signierte Mac-Version, sobald das Signieren finanziert ist; bis dahin gibt es einen experimentellen Download

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
