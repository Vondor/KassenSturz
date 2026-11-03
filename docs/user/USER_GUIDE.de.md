# KassenSturz: Benutzerhandbuch

*Für alle, die KassenSturz zum ersten Mal benutzen. Zum Lesen und Nachschlagen. Alles in diesem Handbuch klappt mit erfundenen Zahlen genauso wie mit echten; zum Ausprobieren nehmen Sie bitte ein Beispielbuch (Abschnitt 2). Eine genauere Beschreibung aller Seiten steht in [NOTES.md](NOTES.md) (englisch). Die Beschriftungen unten sind die der deutschen App.*

## Inhalt

1. [Installieren](#1-installieren)
2. [Der erste Start](#2-der-erste-start)
3. [Die Übersicht](#3-die-übersicht)
4. [Konten und Kategorien](#4-konten-und-kategorien)
5. [Buchungen eintragen, ändern, aufteilen](#5-buchungen-eintragen-ändern-aufteilen)
6. [Geld zwischen eigenen Konten umbuchen](#6-geld-zwischen-eigenen-konten-umbuchen)
7. [Rückgängig machen und Verlauf](#7-rückgängig-machen-und-verlauf)
8. [Einen Kontoauszug von der Bank einlesen](#8-einen-kontoauszug-von-der-bank-einlesen)
9. [Regeln: die App sortiert für Sie](#9-regeln-die-app-sortiert-für-sie)
10. [Ihre Daten sichern](#10-ihre-daten-sichern)
11. [Wenn etwas nicht klappt](#11-wenn-etwas-nicht-klappt)

---

## 1. Installieren

Die Schritte stehen mit allen Klicks auf einem eigenen Blatt für Ihr System:

- **Windows:** [Installationsblatt Windows](../testing/INSTALL_WINDOWS.de.md)
- **Ubuntu:** [Installationsblatt Ubuntu](../testing/INSTALL_UBUNTU.de.md)

Kurz gesagt:

1. Öffnen Sie **https://github.com/Vondor/KassenSturz/releases** im Browser. Ein GitHub-Konto brauchen Sie nicht.
2. Laden Sie bei der obersten Version unter **Assets** die Datei für Ihr System herunter (Windows: die Datei mit „windows“ im Namen, Ubuntu: die Datei, die auf `.flatpak` endet).
3. Öffnen Sie sie und folgen Sie dem Installationsblatt. Windows warnt bei einer Testversion einmal („Der Computer wurde durch Windows geschützt“); das Blatt zeigt, wie Sie das bestätigen.

Ihre Bücher bleiben bei einer neuen Version erhalten. Eine neuere Version installieren Sie einfach über die alte.

## 2. Der erste Start

Beim ersten Start führt Sie ein kurzer Einrichtungsassistent durch das Nötigste. Sie können jederzeit zurückgehen, und es wird erst etwas gespeichert, wenn Sie den Schritt abschließen. Schließen Sie das Fenster mittendrin, beginnt der Assistent beim nächsten Mal neu.

1. **Willkommen bei KassenSturz:** Sie wählen Ihre **Sprache** (die Ihres Systems ist schon gewählt) und KassenSturz fragt, welche **Währung** Sie verwenden. Meistens bestätigen Sie nur mit **Weiter**. Ändern Sie die Sprache, startet KassenSturz einmal neu in dieser Sprache; es geht nichts verloren.
2. **Wie möchten Sie anfangen?** Sie haben drei Möglichkeiten: **Mit meinen eigenen Konten** (etwa zwei Minuten), **Ein Beispielbuch ausprobieren** (ein ausgedachter Haushalt zum Umsehen) oder **Meine Daten aus einem anderen Programm holen** (ein KMyMoney-Buch oder eine mit KassenSturz gespeicherte Kopie). Wer schon ein KassenSturz-Buch hat, wählt den kleinen Link **Ein KassenSturz-Haushaltsbuch öffnen, das ich schon habe…**.
3. Bei **Mit meinen eigenen Konten** legen Sie Ihre Konten an: Art (Girokonto, Sparkonto, Bargeld, Kreditkarte), Name und wie viel heute darin ist. Nehmen Sie den Betrag aus Ihrer Banking-App oder vom Kontoauszug, damit KassenSturz von dem ausgeht, was wirklich da ist. **Zum Ausprobieren dürfen Sie sich einen Betrag ausdenken.** Leer lassen heißt: nichts drin. Mit **Haushaltsbuch anlegen** wird alles auf einmal gespeichert.
4. Dann fragt KassenSturz, wie neue Buchungen hineinkommen: **Ich tippe sie selbst ein**, **Eine Datei von meiner Bank importieren** oder **Mit meiner Bank verbinden**. Auch das lässt sich später ändern.
5. Dann sehen Sie die **Übersicht** mit einer kurzen Meldung, dass Ihr Haushaltsbuch bereit ist.

KassenSturz legt für Sie **Kategorien** an (Einnahmen wie Gehalt, Ausgaben wie Wohnen, Essen, Gesundheit). Sie gehören Ihnen: umbenennen, verschieben und ergänzen dürfen Sie alles.

**Lieber erst mit einem Beispielbuch üben?** Wählen Sie **Ein Beispielbuch ausprobieren** (beim ersten Start oder später unter **Anderes Haushaltsbuch öffnen…**). Zur Auswahl stehen *Vorstadtfamilie* (empfohlener Einstieg, ein ganz normaler Haushalt), *Blumen-Utopia* (eine kleine Gartengenossenschaft), *Weltraumkolonie* (fünf Jahre in einer Marskolonie) und *UdSSR-Agent* (ein ausgedachter Agent der 1980er mit mehreren Währungen). Es entsteht eine eigene Kopie; das mitgelieferte Buch bleibt unverändert.

**Ihr Haushaltsbuch** legt KassenSturz selbst in einem eigenen Ordner an und öffnet es bei jedem Start wieder. Sie müssen keine Datei auswählen. Unter *Einstellungen* können Sie ein anderes Buch öffnen (**Anderes Haushaltsbuch öffnen…**) oder ein neues anlegen (**Neues Haushaltsbuch anlegen…**), zum Beispiel zum Ausprobieren. Ein Buch ist immer nur an einem Ort geöffnet; Bücher auf Netzlaufwerken meldet KassenSturz mit einer Warnung.

## 3. Die Übersicht

Die **Übersicht** ist die Startseite. Sie zeigt, wie Sie dastehen:

- was alle Konten **zusammen** heute enthalten,
- für einen Monat **Geld rein**, **Geld raus** und was **übrig** bleibt (oder „Mehr ausgegeben als hereingekommen“),
- **Wohin Ihr Geld ging** und **woher es kam**, nach Kategorien, die größten zuerst. Was Sie noch nicht sortiert haben, steht unter **Noch nicht zugeordnet**.

Mit den Pfeilen neben dem Monatsnamen blättern Sie in andere Monate. **Zurück zu diesem Monat** bringt Sie wieder in den jetzigen.

Wichtig zu wissen: **Geld, das Sie zwischen eigenen Konten umbuchen**, zählt nicht als Geld rein oder raus (Abschnitt 6). Haben Sie Konten in mehreren Währungen, hat jede Währung ihre eigenen Summen; KassenSturz zählt Euro und Dollar nie zusammen. Die Übersicht lässt sich an Ihre Wünsche anpassen: weitere Diagramme und eigene Dashboards.

## 4. Konten und Kategorien

**Konten** (im Menü links) zeigt Ihre Konten nebeneinander mit dem, was drin ist. Bei einer Kreditkarte steht „Sie schulden …“. Ab zwei Konten zeigt „Alles zusammen“, was Sie haben minus was Sie schulden.

- **Konto hinzufügen:** Art wählen, Namen geben, Betrag und Tag angeben.
- **Ein Konto öffnen:** Karte anklicken. Sie sehen alle Buchungen. Rechts oben (**Über dieses Konto**) können Sie es umbenennen, mit der Bank abgleichen oder schließen. Schließen geht nur, wenn nichts mehr drauf ist; die Geschichte bleibt.
- **Mit der Bank abgleichen…:** Tippen Sie ein, was Ihre Bank an einem Tag als Kontostand nennt. KassenSturz sagt, ob es stimmt. Wenn nicht, zeigt es den Unterschied genau und nennt mögliche Gründe (eine doppelte oder fehlende Buchung, ein falscher Anfangsstand). Es ändert nichts von allein.

**Kategorien** (im Menü links) zeigt, woher Geld kommt und wofür es ausgegeben wird, als Baum (zum Beispiel *Lebensmittel* innerhalb von *Essen*). Klicken Sie eine Kategorie an, um sie umzubenennen oder zu verschieben. **Kategorie hinzufügen** legt eine neue an.

## 5. Buchungen eintragen, ändern, aufteilen

1. Öffnen Sie das Konto und wählen Sie **Buchung hinzufügen**.
2. Sagen Sie, was passiert ist: **Ich habe bezahlt** oder **Ich habe Geld erhalten**, wie viel, von wem oder an wen, wofür und wann. Beträge tippen Sie so, wie Ihre Einstellungen sie zeigen, in Deutschland also zum Beispiel `53,20`.
3. **Noch nicht zugeordnet** lässt die Kategorie offen; Sie können sie später zuordnen.
4. Speichern. Eine Meldung sagt, was getan wurde, mit einem Knopf **Rückgängig**.

Ändern oder löschen: Buchung anklicken. Löschen nimmt sie aus der Liste und aus dem Kontostand; *Rückgängig* bringt sie zurück.

In der Liste sortieren Sie über **Sortiert nach** (Datum, Betrag, Name, Kategorie). Das Feld **Diese Buchungen filtern** über der Liste zeigt nur Buchungen, die alle Ihre Suchwörter enthalten.

**Eine Buchung aufteilen:** Eine Rechnung im Supermarkt kann aus Lebensmitteln und Haushaltswaren bestehen. Öffnen Sie die Buchung und wählen Sie **Diese Buchung aufteilen**. Fügen Sie Teile hinzu und sagen Sie bei jedem, wofür er war. Ein Teil darf leer bleiben, KassenSturz rechnet den Rest aus. Die Teile müssen zusammen die ganze Buchung ergeben; sonst sagt die Seite, um wie viel es abweicht.

## 6. Geld zwischen eigenen Konten umbuchen

Legen Sie 200 € vom Girokonto aufs Sparkonto, ist das weder Ausgabe noch Einnahme. So geht es:

1. Tragen Sie im Girokonto **Ich habe bezahlt** 200 € ein und im Sparkonto **Ich habe Geld erhalten** 200 €, beide mit **Noch nicht zugeordnet**.
2. Nach der zweiten Buchung fragt KassenSturz: **Geld zwischen Ihren Konten bewegt?** Antworten Sie **Ja, verbinden**. Dann zählt es weder als Ausgabe noch als Einnahme.

Die beiden Seiten müssen gleich hoch sein, in derselben Währung, höchstens drei Werktage auseinander und in verschiedenen Konten. Bei **Nein** bleiben sie getrennt. *Rückgängig* trennt eine Verbindung wieder.

## 7. Rückgängig machen und Verlauf

- **Rückgängig** und **Wiederholen** stehen im Menü und auf **Strg+Z** und **Strg+Umschalt+Z**. Sie sagen, was sie tun („Rückgängig: Eine Buchung entfernt“). Rückgängig nimmt immer die neueste Änderung zurück, auch nachdem Sie das Buch geschlossen und wieder geöffnet haben.
- Unter *Einstellungen* zeigt **Verlauf anzeigen** die letzten 100 Änderungen mit Tag und Uhrzeit.

Sie können also nichts so ändern, dass es nicht mehr zurückgeht. Auch vor Aktualisierungen legt KassenSturz eine Sicherung des Buches an (*Einstellungen › Sicherungen*) und behält die letzten zehn.

## 8. Einen Kontoauszug von der Bank einlesen

1. Laden Sie in Ihrem Online-Banking einen Kontoauszug herunter: **CAMT.053** (.xml), **MT940** (.sta), **OFX** (.ofx) oder **CSV** (.csv, .txt).
2. Öffnen Sie in KassenSturz das passende Konto und wählen Sie **Von meiner Bank importieren**.
3. Bei einer **CSV-Datei** fragt KassenSturz, wie sie zu lesen ist (Trennzeichen, Spalten, Datumsform …). Die Vorschau zeigt sofort, ob Ihre Wahl stimmt. Die Wahl wird für die nächste Datei derselben Bank gemerkt.
4. Die **Vorschau** sagt, **bevor etwas passiert**: wie viele Buchungen neu sind, wie viele schon im Buch stehen (die werden nicht doppelt übernommen), wie viele einer vorhandenen Buchung ähneln und wie viele Ihre Regeln gleich sortieren. Lesen Sie sie, dann **Importieren**, oder **Nicht importieren**.
5. Danach sagt KassenSturz, ob der Kontostand mit dem der Datei übereinstimmt.

*Rückgängig* nimmt den ganzen Import zurück.

## 9. Regeln: die App sortiert für Sie

Mit **Regeln** (im Menü) sortiert KassenSturz neue Buchungen selbst, zum Beispiel alles vom Supermarkt in *Lebensmittel*. Haben Sie dieselbe Gegenseite dreimal von Hand in dieselbe Kategorie sortiert, schlägt KassenSturz selbst eine Regel vor (**Regel erstellen** oder **Nein, danke**). Eine Regel legen Sie mit **Regel hinzufügen** an: Name, „Wenn eine Buchung passt“ (eine oder mehrere Bedingungen), „Dann“ (in eine Kategorie sortieren). Sie sehen schon beim Tippen, wie viele Buchungen passen. Die erste passende Regel in der Liste gilt; mit den Pfeilen ändern Sie die Reihenfolge. Eine Regel sortiert nur Buchungen ohne Kategorie, Ihre eigene Sortierung bleibt unberührt.

## 10. Ihre Daten sichern

Unter *Einstellungen › Ihre Daten* gibt es **Eine Kopie meiner Daten speichern**:

- **Für eine Tabelle:** die Buchungen der gewählten Konten als Datei, die Excel oder LibreOffice Calc öffnet.
- **Das ganze Haushaltsbuch speichern:** alles in einer Datei, aus der KassenSturz ein neues Buch anlegen kann, zum Beispiel als Sicherung.

KassenSturz überschreibt dabei nie eine vorhandene Datei. Ihre Daten bleiben auf Ihrem Computer; ohne Ihre ausdrückliche Zustimmung wird nichts versendet.

Bücher aus **KMyMoney** (.kmy oder SQLite) können Sie einlesen, ebenso eine früher mit KassenSturz gespeicherte Datei.

## 11. Wenn etwas nicht klappt

- **Jede Meldung sagt, was passiert ist und was Sie tun können.** Lesen Sie sie ganz; meistens steht die Lösung darin.
- **„Dieses Buch ist auf … geöffnet“** heißt: Es ist an einem anderen Ort offen. Schließen Sie es dort und wählen Sie **Erneut versuchen**.
- **Ein Wert wird nicht angenommen** („Das ist kein Betrag“): Tippen Sie ihn so, wie die Meldung es zeigt, in Deutschland also `1.234,56`.
- **Fehler gefunden?** Beschreiben Sie, was Sie tun wollten, was Sie erwartet haben und was passiert ist, mit der Versionsnummer aus dem Fenster **Über KassenSturz**. Tragen Sie dabei keine echten Kontodaten in Bildschirmfotos ein, die Sie weitergeben.
- **Ein Test-Hinweis:** Nehmen Sie Beispielbücher und erfundene Zahlen, solange KassenSturz im Test ist. Bücher aus älteren Entwicklungsversionen öffnen sich nicht in neueren; sichern Sie vorher das ganze Buch (Abschnitt 10).

KassenSturz ist ein Werkzeug zum Führen Ihres Haushaltsbuchs. Es gibt keine Steuer-, Rechts- oder Anlageberatung.
