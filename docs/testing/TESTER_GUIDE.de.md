# KassenSturz ausprobieren: Der Tester-Guide

*Für Testerinnen und Tester. Dauer etwa 45 Minuten. Englische Fassung: [TESTER_GUIDE.en.md](TESTER_GUIDE.en.md).*

Vielen Dank, dass Sie KassenSturz ausprobieren. KassenSturz ist ein freies Programm für die Haushaltskasse auf dem Computer. Es soll so einfach sein, dass man dafür weder Buchhaltung noch Wirtschaft gelernt haben muss.

## Was passiert beim Test?

Vor dem Termin installieren Sie KassenSturz zu Hause, allein, mit dem Installationsblatt und dem [Installationsprotokoll](INSTALL_LOG.de.md). Der Termin selbst dauert etwa 45 bis 60 Minuten. Die Testleitung ist die Person, die Sie eingeladen hat. Sie sitzt neben Ihnen oder ist per Videoanruf dabei. Sie gibt Ihnen **Aufgabenkarten**, eine nach der anderen. Sie lösen sie mit KassenSturz und denken dabei laut. Am Ende beantworten Sie ein paar Fragen.

| Wann | Was | Etwa |
|---|---|---|
| Vorher, zu Hause | Dieses Blatt lesen, KassenSturz installieren (Karte 1), Protokoll schicken | etwa 20 Min. |
| Termin | Die Karten 2 bis 5, 7 und 8 | 30 Min. |
| Danach | Fragen zum Schluss | 10 Min. |

## 1. Sie können hier nichts falsch machen

**Wir testen das Programm, nicht Sie.** Wenn Sie etwas nicht finden, nicht verstehen oder sich ärgern, ist das kein Fehler von Ihnen. Es ist genau der Fund, den wir suchen. Ein Test, bei dem alles glatt läuft, bringt uns fast nichts.

- **Sie verletzen niemanden.** Die Leute, die KassenSturz bauen, wollen ehrliche Kritik, auch harte. „Das ist doof“, „Das verstehe ich nicht“ und „Warum ist das hier?“ sind die wertvollsten Sätze des ganzen Tests.
- **Sie können nichts kaputt machen.** Sie arbeiten mit erfundenen Zahlen. Was Sie tun, betrifft nichts Echtes.
- **Es gibt keine dummen Gedanken.** Auch ein kurzes „Hm“ oder „Moment …“ hilft uns.
- **Aufgeben ist erlaubt.** Kommen Sie nicht weiter, sagen Sie es, und gehen Sie zur nächsten Karte. Auch das ist ein wichtiger Fund.
- **Sie dürfen jederzeit eine Pause machen oder aufhören**, ohne einen Grund zu nennen.

## 2. Laut denken: so geht es

Beim lauten Denken sagen Sie **alles, was Ihnen durch den Kopf geht**, während Sie das Programm benutzen. Stellen Sie sich vor, Sie erklären einem Freund am Telefon, was Sie gerade sehen und tun, und der Freund sieht Ihren Bildschirm nicht.

Zum Beispiel:

> „Ich suche jetzt, wo ich das Einkaufen eintrage … da oben steht ‚Hinzufügen‘, das probiere ich mal … hm, das ist ein Konto, keine Ausgabe … ich weiß nicht, was ‚Teilbetrag‘ heißt … ah, jetzt hat es geklappt, aber ich bin nicht sicher, ob es gespeichert ist.“

**Warum wir das wollen:** Von außen sehen wir nur, wo Sie klicken. Warum Sie dort klicken, was Sie erwartet haben und was Sie verwirrt hat, erfahren wir nur, wenn Sie es aussprechen. Genau das ist es, was wir verbessern können.

So geht es gut:

- Sprechen Sie **ununterbrochen**, auch wenn es sich am Anfang komisch anfühlt. Nach zwei, drei Minuten wird es normal.
- Sagen Sie, **was Sie erwarten**, bevor Sie klicken („Ich glaube, da kommt jetzt …“), und **was dann wirklich passiert**.
- Sagen Sie, **was Sie fühlen**: unsicher, genervt, überrascht, erleichtert.
- **Meckern ist ausdrücklich erwünscht.** Wenn Sie etwas nervt, sagen Sie es genau so.
- Werden Sie still, fragt die Testleitung nur: „Was denken Sie gerade?“ Sie hilft nicht und erklärt nichts. Das gehört zum Test, denn zu Hause sitzt auch niemand neben Ihnen.

Die Testleitung übt das vor dem Start kurz mit Ihnen.

## 3. Was sicher ist

> **Bitte keine echten Daten.** Benutzen Sie im Test nur das Beispielbuch. Wenn Sie trotzdem eigene Konten, Beträge, Banken oder IBANs eingeben, werden diese Daten mit aufgenommen und ausgewertet: in der Bildschirm- und Tonaufnahme, im Protokoll der App und in Ihrem Bericht. Deshalb bitten wir Sie, das nicht zu tun.

- **Bitte nur erfundene Zahlen und die Beispielbücher.** Tippen Sie keine eigenen Konten, Beträge, Banknamen oder IBANs ein. Fragt das Programm nach einem Kontostand, denken Sie sich einen aus. Zum Üben gibt es vier fertige Beispielbücher mit ausgedachten Menschen und Zahlen.
- **Nichts verlässt Ihren Computer.** Der Test braucht keine Bank, keinen KI-Dienst und nach der Installation kein Internet. Lassen Sie KI und Bankverbindungen ausgeschaltet.
- **Falls Sie doch echte Daten eingeben:** Das ist kein Problem, bitte sagen Sie es uns ehrlich (Frage 8 am Ende des Fragebogens). Dann behandeln wir Ihren Bericht als persönliche Daten: Er geht nur an die Testleitung, wird nirgends veröffentlicht, nicht ins Projekt, nicht in die öffentliche Fehlerliste und nicht an Programme oder KI-Werkzeuge gegeben, und er wird nach der Testrunde gelöscht.
- **Ihr Haushaltsbuch ist nicht verschlüsselt.** KassenSturz speichert Ihr Haushaltsbuch unverschlüsselt auf Ihrem Computer. Wer Ihren Computer benutzen kann, kann es öffnen. Schützen Sie deshalb Ihr Benutzerkonto mit einem Passwort, und schalten Sie, wenn möglich, die Verschlüsselung Ihres Systems ein (unter Windows „Geräteverschlüsselung“ oder BitLocker).
- **Aufnahme nur mit Ihrer Zustimmung.** Die Testleitung fragt vorher. Ein Nein ist völlig in Ordnung, der Test läuft dann genauso.
- **Ein Hinweis zum Entwicklermodus.** Für den Test starten wir KassenSturz im *KassenSturz-Entwicklermodus*. Dann schreibt das Programm ein Protokoll mit, damit wir sehen, wo es langsam war oder einen Fehler hatte. Es enthält nur Zeiten, Namen von Bildschirmen und Knöpfen und Fehlercodes, **keine Beträge, keine Namen und keine Texte aus Ihrem Buch**. Steht oben im Fenster „Entwicklermodus“, läuft er. Er ist für Tests gedacht, nicht für den Alltag. Lassen Sie die KI dabei ausgeschaltet, sonst würde auch das KI-Gespräch mitgeschrieben.

## 4. Was Sie brauchen

- Einen Computer mit **Windows 11 (oder Windows 10)** oder mit **Ubuntu**. Eine Anleitung, die Sie allein befolgen können, gibt es für beide:
  - [Installationsblatt Windows](INSTALL_WINDOWS.de.md)
  - [Installationsblatt Ubuntu](INSTALL_UBUNTU.de.md)
- Eine Internetverbindung für den Download (danach nicht mehr).
- Bei Ubuntu einmal Ihr Passwort für den Computer.
- Stift und Papier für Notizen, oder eine zweite Datei.

**Die Installation gehört zum Test.** Sie installieren zu Hause, allein, nur mit dem Installationsblatt. Niemand hilft Ihnen dabei, auch nicht am Telefon. Tragen Sie jeden Schritt ins [Installationsprotokoll](INSTALL_LOG.de.md) ein. Kommen Sie nicht weiter, schreiben Sie ins Protokoll, wo, hören Sie dort auf und schicken Sie das Protokoll an die Testleitung. Das ist ein wertvoller Fund. Sie bekommen dann per E-Mail einen Hinweis, damit der Test weitergeht.

## 5. Installieren in Kürze

Die ganze Anleitung mit allen Klicks steht auf dem Installationsblatt. Hier der Überblick, damit Sie vorher wissen, was auf Sie zukommt:

1. **Download-Seite öffnen:** https://github.com/Vondor/KassenSturz/releases (kein GitHub-Konto nötig). Die oberste Version ist die neueste Beta-Version und als *Pre-release* gekennzeichnet. Die Testleitung sagt Ihnen vorher ihre Nummer (zum Beispiel 0.261117.8).
2. **Die richtige Datei laden:** unter **Assets** die Datei für Ihr System. Für Windows die Datei, die auf `-windows-x64-setup.exe` endet. Unter Ubuntu brauchen Sie meist gar keine Datei: Sie installieren KassenSturz mit einem Befehl aus dem Snap Store.
3. **Windows:** Windows warnt zweimal, weil die Beta-Version noch nicht signiert ist („Der Computer wurde durch Windows geschützt“). Das Blatt zeigt, wie Sie trotzdem fortfahren. Meist brauchen Sie keine Administrator-Rechte.
   **Ubuntu:** Sie geben einen Befehl ins Terminal ein und dafür einmal Ihr Passwort. Klappt das nicht, zeigt das Blatt einen zweiten Weg mit einer Flatpak-Datei.
4. **Starten im Entwicklermodus** (Abschnitt 3) und ein Konto mit **ausgedachtem** Kontostand anlegen.

Wie man KassenSturz danach im Alltag benutzt, lesen Sie im [Benutzerhandbuch](../user/USER_GUIDE.de.md). Sie brauchen es für den Test nicht vorher.

## 6. Der Ablauf, Schritt für Schritt

1. Sie lesen dieses Blatt (5 Minuten).
2. Sie installieren KassenSturz zu Hause (Karte 1) und schicken das Installationsprotokoll an die Testleitung.
3. Die Testleitung begrüßt Sie, fragt wegen der Aufnahme und übt das laute Denken mit Ihnen.
4. Sie beantworten die Fragen „Vor dem Test“ ([Fragebogen](QUESTIONNAIRE_1.de.md), Abschnitt 1).
5. Sie bekommen die Aufgabenkarten ab Karte 2 **eine nach der anderen** und lösen sie laut denkend.
6. Sie beantworten die Fragen „Nach dem Test“.
7. Wenn Sie ein Problem gefunden haben, das Sie aufschreiben möchten, benutzen Sie die [Berichtsseite](REPORT_PAGE.de.md). Die schicken Sie nur an die Testleitung.

Vielen Dank, dass Sie dabei sind.
