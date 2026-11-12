# 12. Regeln: die App sortiert für Sie

*Eine Regel sagt KassenSturz: „Sortiere alles von diesem Namen in diese Kategorie.“ Diese Seite zeigt, wie Sie Regeln anlegen, anwenden und zurücknehmen.*

## Wozu Regeln

Mit **Regeln** im Menü sortiert KassenSturz neue Buchungen selbst, zum Beispiel alles vom Supermarkt in *Lebensmittel*. Das spart Handarbeit, vor allem nach einem Import.

## KassenSturz schlägt Regeln vor

Haben Sie denselben Zahlungspartner **dreimal** von Hand in dieselbe Kategorie sortiert, fragt KassenSturz: „Soll KassenSturz das ab jetzt für Sie erledigen?“ Mit **Regel erstellen** öffnet sich der Regel-Editor, schon ausgefüllt; mit **Nein, danke** wird die Frage nicht wiederholt. Das ist einfaches Zählen, keine KI.

Auf einer Buchungsseite macht **Aus dieser Buchung eine Regel erstellen…** dasselbe auf Wunsch.

## Eine Regel anlegen

1. **Regeln** im Menü, dann **Regel hinzufügen**.
2. Geben Sie der Regel einen **Namen**.
3. Bei **Wenn eine Buchung passt** legen Sie eine oder mehrere Bedingungen fest, etwa: der **Zahlungspartner** enthält ein Wort. Beim Tippen sehen Sie, **wie viele Buchungen** passen.
4. Bei **Dann** sagen Sie, was geschieht: in eine Kategorie sortieren. Eine Regel kann auch den **Angezeigten Namen** ersetzen, etwa durch ein klares „Supermarkt“ statt eines Wirrwarrs wie „SB-MARKT 1234 DANKE“.
5. **Regel hinzufügen** speichert.

**Muster (fortgeschritten)** erlauben genauere Bedingungen. Man braucht sie nie; wie sie funktionieren, steht auf der Hilfeseite **Hilfe: Muster, Formeln und Diagramme** (unter einer Muster-Bedingung: **Mehr über Muster**) und auf Seite 15.

## Reihenfolge und Unterregeln

- Die **erste passende Regel** in der Liste gilt. Mit den Pfeilen ändern Sie die Reihenfolge.
- Eine **Unterregel** (**Eine Regel innerhalb dieser hinzufügen…**) wird nur geprüft, wenn ihre Hauptregel schon gepasst hat. Passt sie auch, verfeinert sie das Ergebnis.
- Eine Regel sortiert **nur Buchungen ohne Kategorie**. Was Sie selbst sortiert haben, bleibt unberührt.

## Regeln auf alte Buchungen anwenden

Eine neue Regel ordnet zunächst nur Buchungen zu, die **später** dazukommen. Für Buchungen, die schon im Buch stehen:

- **Alle Regeln anwenden…** oben auf der Regelseite,
- **Jetzt anwenden…** bei einer Regel,
- **Hinzufügen und jetzt anwenden…** im Regel-Editor.

Ein Fenster zeigt zuerst, **wie viele Buchungen zugeordnet werden**, welche bleiben, wie sie sind, und warum, und die neuesten fünf, wie sie danach aussehen. Erst die Schaltfläche mit der Zahl (zum Beispiel **3 Buchungen zuordnen**) ändert etwas. **Abbrechen** ändert nichts.

Normalerweise werden nur Buchungen ohne Kategorie zugeordnet. Mit **Auch neu zuordnen, was Regeln vorher zugeordnet haben** werden auch Buchungen neu zugeordnet, die eine Regel früher zugeordnet hat und die Sie seitdem nicht angefasst haben. **Was Sie selbst zugeordnet oder geändert haben, bleibt immer unberührt.** Einmal **Rückgängig** nimmt alles zurück.

**Wenn etwas anders aussieht:** Wird eine Buchung nicht sortiert, steht sie vielleicht schon in einer Kategorie, oder eine Regel weiter oben hat sie zuerst bekommen. Das Fenster beim Anwenden nennt den Grund.
