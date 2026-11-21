# 23. Plugins

*Ein Plugin ist eine kleine Datei, die KassenSturz um etwas erweitert. Diese Seite sagt, was Plugins sind, wie Sie eines sicher hinzufügen und auf welche das Projekt hinweist. Sie brauchen nie ein Plugin: KassenSturz ist ohne vollständig.*

> **Noch ist nicht alles entschieden.** Wie Plugins ausgewählt, aufgelistet und weitergegeben werden, wird noch festgelegt; diese Seite kann sich also ändern.

## Was ein Plugin ist

Ein Plugin ist eine Datei mit der Endung `.wasm`, die jemand geschrieben hat, um KassenSturz eine weitere Fähigkeit zu geben. Ein Plugin kann:

- eine weitere Art von Bankdatei lesen,
- Ihre Daten in einer weiteren Art von Datei speichern,
- einen Bericht hinzufügen,
- auf neue Buchungen reagieren, zum Beispiel nach einem Import, und Änderungen vorschlagen,
- einen weiteren KI-Dienst anbinden (nur solange die KI-Funktionen eingeschaltet sind),
- die Farben und Schriften ändern oder wie viel die Seiten erklären.

Ein Plugin läuft abgeschottet vom Rest Ihres Computers. Es tut nur, was seine Art erlaubt und was Sie beim Hinzufügen gesehen haben. Es ändert Ihr Buch nie von allein: Eine Änderung, die es vorschlägt, sehen Sie zuerst, und erst Ihre Bestätigung führt sie aus.

## Welche Plugins es gibt

Es gibt keinen Shop und keine Liste in der App. Diese Seite nennt nur Plugins, die das Projekt ausgewählt hat; ein Eintrag kann ein Link zu einer anderen Webseite sein, auf der sein Autor es bereithält.

**Noch ist kein Plugin ausgewählt.** Sobald es die ersten gibt, stehen sie hier, mit dem, was sie tun, wer sie macht und wo Sie sie bekommen.

Ein Plugin, das sich bewährt, kann, wenn gewünscht, Teil von KassenSturz selbst werden. Dann müssen Sie es nicht mehr hinzufügen.

## Ein Plugin hinzufügen

1. Laden Sie die Plugin-Datei auf Ihren Computer, zum Beispiel in den Ordner **Downloads**. Nehmen Sie Plugins nur von Leuten, denen Sie vertrauen.
2. Öffnen Sie in KassenSturz **Einstellungen › Plugins**.
3. Wählen Sie **Plugin-Datei hinzufügen…** und dann die Datei.
4. KassenSturz zeigt **Dieses Plugin hinzufügen?** mit dem, was das Plugin hinzufügt und was es dürfen möchte, zum Beispiel Ihr Buch lesen. Lesen Sie das. Noch ist nichts hinzugefügt.
5. **Hinzufügen** fügt das Plugin hinzu. **Nicht hinzufügen** ändert nichts.

Die heruntergeladene Datei wird nicht verändert, und Sie können sie danach löschen. Fügen Sie ein Plugin hinzu, das Sie schon haben, sagt KassenSturz das und ersetzt das alte.

## Ein Plugin entfernen

Wählen Sie unter **Einstellungen › Plugins › Ihre Plugins** **Dieses Plugin entfernen…**. KassenSturz löscht seine Kopie des Plugins. Ihr Buch wird nicht verändert. Um das Plugin wieder zu benutzen, fügen Sie seine Datei erneut hinzu.

## Ein Plugin schreiben

Das ist etwas für Leute, die programmieren. Die Anleitung ist [Writing plugins](../../WRITING_PLUGINS.md), das Nachschlagewerk [Plugins](../../PLUGINS.md), beide auf Englisch.

**Wenn etwas anders aussieht:** Sagt KassenSturz, ein Plugin könne nicht mehr benutzt werden, wurde es für eine andere Version von KassenSturz gemacht. Entfernen Sie es und suchen Sie nach einer neueren Version davon.
