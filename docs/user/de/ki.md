# Eine KI mit KassenSturz benutzen

*Diese Seite ist für alle, die eine KI nach ihrem Haushaltsbuch fragen möchten und noch nie eine eingerichtet haben. Sie erklärt jedes Wort und jeden Schritt. Sie brauchen nichts davon: KassenSturz ist ohne KI vollständig, und der Rest dieses Handbuchs benutzt sie nie.*

## Was das ist, in einfachen Worten

- Eine **KI** ist hier ein Programm, dem Sie Fragen in ganz normalen Sätzen stellen, zum Beispiel „Wie viel habe ich letzten Monat für Lebensmittel ausgegeben?“, und das in Sätzen antwortet.
- Der Teil, der antwortet, heißt **Modell**. Ein Modell ist eine große Datei. Es gibt viele Modelle, manche sind besser als andere.
- Ein **Anbieter** ist das, was das Modell für Sie laufen lässt. Das kann ein Programm auf Ihrem eigenen Computer sein oder ein Dienst im Internet.
- KassenSturz erreicht den Anbieter über eine **Adresse**, eine Textzeile ähnlich der Adresse einer Webseite.
- Ein **Schlüssel** ist eine Art Passwort, das manche Internetdienste Ihnen geben. Ein Programm auf Ihrem eigenen Computer braucht keinen.

In KassenSturz ist die KI **aus**, bis Sie sie einschalten. Solange sie aus ist, wird nichts davon gezeigt, und nichts davon läuft.

## Was die KI kann und was nicht

- Sie kann Fragen zu Ihrem Buch beantworten und Änderungen vorbereiten, zum Beispiel Kategorien oder Regeln vorschlagen.
- **Sie ändert nichts von allein.** Jede Änderung sehen Sie zuerst, und erst Ihr Klick auf **Bestätigen und diese Änderungen vornehmen** führt sie aus. **Rückgängig** nimmt sie zurück.
- **Die Zahlen kommen von KassenSturz, nicht von der KI.** Die KI fragt KassenSturz nach Ihren Summen und Kontoständen. Prüfen Sie eine Zahl, auf die es ankommt, trotzdem auf den Seiten von KassenSturz selbst.
- Sie gibt keine Steuer-, Rechts- oder Anlageberatung, auf die Sie sich verlassen sollten.
- Sie sieht nie Ihre Banking-PIN oder Ihren Bankzugang.

## Welcher Weg passt zu Ihnen?

| Sie möchten | Nehmen Sie | Was Ihren Computer verlässt |
|---|---|---|
| Alles auf dem eigenen Computer behalten | **Weg 1:** ein Programm auf Ihrem Computer | Nichts |
| Einen bezahlten Internetdienst nutzen, für den Sie einen Schlüssel haben | **Weg 2:** ein Anbieter im Internet | Ihre Fragen und die Teile Ihres Buchs, die für die Antwort nötig sind; KassenSturz zeigt Ihnen genau, was, und fragt vorher |
| Ein KI-Programm nutzen, das Sie schon haben, etwa eine KI-App für den Schreibtisch, die „MCP“ beherrscht | **Weg 3:** Ihr eigenes KI-Werkzeug | Was dieses Werkzeug weiterschickt; KassenSturz kann das weder sehen noch begrenzen |

Wenn Sie unsicher sind, nehmen Sie Weg 1.

## Weg 1: eine KI auf Ihrem eigenen Computer

Sie brauchen einen einigermaßen neuen Computer. Mit einer Grafikkarte mit 8 GB Speicher oder mehr kommen die Antworten in Sekunden statt in Minuten. Wenn Sie nicht wissen, was Ihr Computer hat, probieren Sie es einfach aus: Es kann nichts kaputtgehen.

Dieser Weg benutzt **LM Studio**, ein Programm, das Modelle herunterlädt und auf Ihrem Computer laufen lässt. Es lässt sich kostenlos herunterladen. (Ollama ist ein anderes solches Programm; seine Adresse steht in der [längeren Anleitung](../CONNECT_AN_AI.md), auf Englisch.)

### Schritt 1: LM Studio installieren

1. Öffnen Sie Ihren Browser, also das Programm, mit dem Sie Webseiten ansehen.
2. Tippen Sie `lmstudio.ai` oben in die Adresszeile und drücken Sie die Eingabetaste.
3. Die Seite bietet einen Download für Ihr System an (Windows, Linux oder Mac). Klicken Sie darauf und warten Sie, bis die Datei in Ihrem Ordner **Downloads** angekommen ist.
4. Öffnen Sie die Datei und folgen Sie dem, was auf dem Bildschirm steht, wie bei jedem Programm, das Sie installieren.
5. Starten Sie **LM Studio**. Das Programm ist auf Englisch.

### Schritt 2: ein Modell holen

1. Suchen Sie in LM Studio die Stelle, an der Sie nach Modellen suchen. Vielleicht bietet es Ihnen beim Start von selbst ein erstes Modell an.
2. Wählen Sie ein Modell mit dem Zeichen **Tool use**. Das ist wichtig: Nur ein solches Modell kann KassenSturz nach Ihren Zahlen fragen. Ein Modell ohne dieses Zeichen antwortet nur aus einer kurzen Übersicht.
3. In unseren Tests mit einem ausgedachten Haushaltsbuch haben Modelle mit etwa 9 Milliarden Parametern (im Namen steht „9B“) gut funktioniert und auf eine Grafikkarte mit 8 GB gepasst. Kleinere machten mehr Fehler, viel größere waren sehr langsam.
4. Laden Sie das Modell herunter. Es ist mehrere Gigabyte groß, das dauert also eine Weile.

### Schritt 3: das Modell mit genug Arbeitsspeicher laden

Wenn Sie das Modell in LM Studio laden, zeigt es die Einstellung **Context Length**. Das ist, wie viel Text das Modell auf einmal im Blick behalten kann. Stellen Sie **32768 oder mehr** ein; 65536 ist besser, wenn Ihr Computer das schafft. Mit dem kleinen Wert, mit dem LM Studio beginnt, verliert die KI den Faden und antwortet falsch oder gar nicht.

### Schritt 4: den lokalen Server von LM Studio starten

Ein „Server“ ist hier nur der Teil von LM Studio, über den andere Programme auf Ihrem Computer, zum Beispiel KassenSturz, mit dem Modell sprechen. Öffnen Sie in LM Studio den Reiter **Developer** und starten Sie dort den Server. Lassen Sie LM Studio offen, solange Sie die KI in KassenSturz benutzen.

### Schritt 5: KassenSturz sagen, wo die KI ist

1. Öffnen Sie in KassenSturz **Einstellungen** im Menü links.
2. Schalten Sie **KI-Funktionen** ein.
3. Wählen Sie **KI-Anbieter und Datenschutz…**.
4. Tippen Sie unter **Adresse** genau: `http://localhost:1234/v1`
   „localhost“ heißt „dieser Computer“. Eine grüne Zeile sagt, dass die Adresse richtig aussieht; eine rote sagt, was nicht stimmt.
5. Wählen Sie beim Schlüssel **Der Anbieter braucht keinen Schlüssel**.
6. Drücken Sie **Speichern**. KassenSturz fragt LM Studio, welche Modelle es hat; wählen Sie Ihres unter **Modell** und drücken Sie noch einmal **Speichern**.
7. Drücken Sie **Verbindung testen**. Das schickt nichts aus Ihrem Buch und sagt Ihnen, ob das Modell die Funktionen von KassenSturz benutzen kann.

Wenn Sie am Ende der geführten Einrichtung (Seite 3) mit Ja geantwortet haben, sind Sie schon auf dieser Seite bei Punkt 4.

### Schritt 6: sagen, was die KI darf, und fragen

Auf derselben Seite ist unter **Was die KI darf** zunächst jede Erlaubnis ausgeschaltet: Die KI liest und ändert nichts in Ihrem Buch, bis Sie es erlauben, Zeile für Zeile. Schalten Sie ein, womit Sie einverstanden sind. Für Fragen genügt das Lesen.

Wählen Sie dann oben im Fenster **KI fragen** und tippen Sie Ihre Frage. Eine Frage braucht auf einem Computer mit passender Grafikkarte meist 20 bis 90 Sekunden.

## Weg 2: ein Anbieter im Internet

Manche Firmen lassen Modelle für Sie laufen und berechnen jede Nutzung. Von ihnen bekommen Sie eine Adresse und einen Schlüssel. Unter **Einstellungen › KI-Anbieter und Datenschutz…** tippen Sie die Adresse, fügen den Schlüssel ein und drücken **Speichern**.

Weil der Anbieter auf einem anderen Computer ist, listet die Seite unter **Was diesen Computer verlässt** auf, was gesendet wird und was nie. Es wird nichts gesendet, bis Sie anhaken, dass Sie es gelesen haben und zustimmen, und vor jeder Frage zeigt der Chat noch einmal genau, was er sendet.

Zwei Dinge werden oft verwechselt:

- Ein Monatsabo für eine Chat-Webseite ist meist **kein** Schlüssel und funktioniert hier nicht. Ein Schlüssel wird getrennt verkauft und nach Nutzung abgerechnet.
- Ein Modell auf einem anderen Computer bei Ihnen zu Hause zählt auch als „anderer Computer“. KassenSturz fragt genauso.

Die Adressen gängiger Anbieter stehen in der [längeren Anleitung](../CONNECT_AN_AI.md) (Englisch).

## Weg 3: Ihr eigenes KI-Werkzeug liest Ihr Buch

Wenn Sie schon ein KI-Programm auf Ihrem Computer benutzen, das „MCP“ beherrscht (ein Weg, auf dem KI-Programme andere Programme nach Informationen fragen), kann es KassenSturz nach dem fragen, was es braucht. Dafür sind ein paar Zeilen in den Einstellungen dieses Programms nötig; der Befehl lautet `kassensturz.mcp` bei der Snap-Store-Version, `kassensturz-mcp` bei der Microsoft-Store-Version und `flatpak run --command=kassensturz-mcp io.github.vondor.KassenSturz` beim Flatpak. Die Zeilen für jede Version stehen in der [längeren Anleitung](../CONNECT_AN_AI.md) (Englisch). Zunächst darf es nichts; Sie erlauben das Lesen und, wenn Sie möchten, das Vorbereiten von Änderungen unter **Was die KI darf**. Jede vorbereitete Änderung wartet weiter auf Ihren Klick in KassenSturz.

## Wieder ausschalten

**Einstellungen**, dann **KI-Funktionen** ausschalten. Alles zur KI verschwindet wieder aus KassenSturz, und nichts davon läuft.

**Wenn etwas anders aussieht:** Zeigt der Chat einen Fehler, sagt die Meldung, was passiert ist und was zu tun ist; oft hat sie einen Knopf **KI-Einstellungen öffnen**. Sagt die Meldung, der Arbeitsspeicher des Modells sei zu klein, gehen Sie zurück zu Schritt 3.
