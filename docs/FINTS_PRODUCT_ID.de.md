# Umgang mit einer FinTS-Produktregistrierungsnummer

*English: [FINTS_PRODUCT_ID.md](FINTS_PRODUCT_ID.md)*

Diese Notiz ist für Entwicklerinnen und Entwickler anderer Open-Source-Programme, die über FinTS (HBCI) mit deutschen Banken sprechen, zum Beispiel über AqBanking. Sie erklärt, wie KassenSturz mit seiner FinTS-Produktregistrierungsnummer umgeht und warum, damit Sie es im eigenen Projekt genauso machen können. Sie enthält keine Nummer und keinen Teil davon.

Sie ist keine Rechtsberatung. Was Sie dürfen, bestimmen die Bedingungen **Ihrer** Registrierung. Fragen Sie im Zweifel die Registrierungsstelle, bevor Sie etwas veröffentlichen.

## Was die Nummer ist

Seit 2019 antworten Banken nur FinTS-Programmen, die bei der Deutschen Kreditwirtschaft registriert sind. Die Registrierung ist kostenlos, gilt einmal pro Produkt (nicht pro Version) und dauert einige Wochen. Sie erhalten eine Produktregistrierungsnummer (25 Buchstaben und Ziffern). Ihr Programm schickt sie in jedem Dialog an die Bank, damit die Bank weiß, welches Produkt mit ihr spricht. Registrierung: <https://www.fints.org/de/hersteller/produktregistrierung>.

Die Nummer ist kein Passwort. Mit ihr kann niemand ein Konto eröffnen oder sich anmelden. Sie kennzeichnet aber Ihr Produkt, und wird sie missbraucht, kann die Registrierungsstelle sie sperren. Dann erreicht kein Nutzer Ihres Produkts mehr seine Bank.

## Regel 1: ein Produkt, eine Nummer

- **Jedes Produkt braucht eine eigene Registrierung.** Verwenden Sie nie eine Nummer, die Sie in einem anderen Programm, in dessen Quelltext, in einem Forum oder in Beispielen einer Bibliothek gefunden haben. Das gilt auch für die Nummer von KassenSturz.
- **Ein geänderter Build ist ein anderes Produkt.** So versteht KassenSturz die Registrierungsstelle: Ein fertiges Programm, das unverändert als das registrierte Produkt ausgeliefert wird, darf die Nummer enthalten; ein Fork oder ein veränderter Build ist ein anderes Produkt, und wer ihn macht, registriert ein eigenes.
- **Für Bibliotheken registrierte Nummern sind nur für interne Tests** gedacht (siehe die FAQ auf fints.org), nicht für ein Programm, das an Nutzer geht.

## Regel 2: die Nummer bleibt aus dem öffentlichen Quelltext

KassenSturz wurde gesagt, dass die Nummer nicht im öffentlichen Quelltext stehen darf. Deshalb enthält keine Datei im Repository sie, und sie gelangt nur in die offiziellen Releases. So ist das gebaut, falls Sie die Idee übernehmen möchten (der Code steht unter GPL-3.0-or-later, siehe `crates/kassensturz-bank/build.rs`):

1. **Der Quelltext hat einen leeren Platz.** Das Programm liest die Nummer beim Kompilieren mit Rusts `option_env!`. Ohne sie ist der Wert `None`.
2. **Das Build-Skript nimmt die Nummer nur von außerhalb des Repositorys:** aus einer Umgebungsvariable oder aus einer Datei im Hauptordner des Repositorys, die in `.gitignore` steht. Es entfernt Leerzeichen, prüft die Form (genau 25 ASCII-Buchstaben und -Ziffern) und gibt die Nummer dem Compiler als Umgebungsvariable (`cargo:rustc-env`). Es schreibt sie nie in eine erzeugte Quelldatei, damit ein Paket mit Debug-Quellen sie nicht mitnimmt.
3. **Eine falsche Form bricht den Build ab**, mit einer Meldung, die den Wert nicht zeigt.
4. **Nur der Release-Job eines Release-Tags bekommt die Nummer.** Er liest sie aus einem Secret, das in einer geschützten Deployment-Umgebung beim Code-Hoster liegt, schreibt die von git ignorierte Datei direkt vor dem Build und löscht sie direkt danach.
5. **Der Release-Job prüft sein eigenes Ergebnis.** Nach dem Build durchsucht er jede Datei des fertigen Pakets und schlägt fehl, wenn die Nummer nicht in der Programmdatei oder zusätzlich in einer anderen Datei steht.
6. **Pull Requests beweisen, dass die Prüfung wirkt**, indem sie dieselben Schritte mit einer erfundenen Nummer der richtigen Form durchlaufen.

### Den Release-Weg schützen

Ein Secret in der CI ist nur so sicher wie der Workflow, der es lesen kann. KassenSturz:

- lässt nur den Maintainer Release-Tags anlegen (ein Tag-Ruleset);
- hält das Secret in einer Deployment-Umgebung, die nur Release-Tags zulässt und den Maintainer jeden Lauf freigeben lässt;
- verwendet nie `pull_request_target`, damit Code aus einem Pull Request nie mit Secrets läuft;
- pinnt jede CI-Action auf einen Commit;
- speichert im Release-Job keinen Build-Cache und läuft nie mit Shell-Tracing (`set -x`) oder sehr ausführlicher Cargo-Ausgabe (`cargo -vv`), die Umgebungsvariablen des Compilers ausgeben würde;
- schreibt keine AqBanking-Nachrichtenprotokolle, denn darin stünde der Dialog, zu dem die Nummer gehört.

Bedenken Sie: Aus einem veröffentlichten Programm kann jeder die Nummer auslesen. Dass sie nicht im Quelltext steht, macht sie nicht geheim; es verhindert, dass sie versehentlich in andere Produkte kopiert wird, und hält Sie innerhalb der Bedingungen Ihrer Registrierung. Planen Sie für eine gesperrte Nummer vor: neu registrieren, die neue Nummer hinterlegen und ein neues Release taggen.

## Wie sich Builds ohne Nummer verhalten

Die meisten Builds haben keine Nummer: Builds aus dem Quelltext, Pull-Request-Builds und Entwicklungs-Builds. Sie müssen trotzdem funktionieren.

- **Die ganze App funktioniert**, auch der Import von Kontoauszugsdateien. Nur der direkte Bankzugang ist aus.
- **Die Bank-Seiten sagen klar**, dass dieser Build nicht registriert ist und was man stattdessen tun kann (eine Datei importieren oder ein offizielles Release verwenden).
- **Für Entwickler, die gegen ihre eigene Bank testen**, hat KassenSturz einen Entwickler-Schalter (eine Umgebungsvariable, nur im README beschrieben). Damit fragt die App in einem Passwortfeld nach einer Nummer, sobald sie zum ersten Mal eine Bank erreichen würde, bevor irgendetwas gesendet wird. Die Nummer wird geprüft, nur im Speicher gehalten, beim Freigeben überschrieben, nie protokolliert oder gespeichert und beim Schließen der App vergessen. „Abbrechen" kontaktiert keine Bank.
- **Wer KassenSturz mit einer eigenen Registrierung baut**, legt seine eigene Nummer in die von git ignorierte Datei und baut. Siehe „Bank access in your own build" im README.

## Die Nummer an AqBanking übergeben

AqBanking nimmt die Nummer als Laufzeit-Einstellung `fintsRegistrationKey` einer offenen Sitzung:

```c
AB_Banking_RuntimeConfig_SetCharValue(ab, "fintsRegistrationKey", key);
```

AqBanking liest diesen Wert bei jeder Nachricht, die es schreibt, und setzt ihn in das Produktfeld des Dialogbeginns. Prüfen Sie die Form selbst, bevor Sie die Nummer übergeben, damit kein FinTS-Trennzeichen in eine Nachricht gelangt.

## Was Sie nicht tun sollten

- Die Nummer nicht in Quelltext, eine erzeugte Datei, einen Test, eine Testdatei, eine Konfigurationsvorlage oder eine Standardeinstellung schreiben.
- Sie nicht in eine Commit-Nachricht, ein Issue, einen Pull Request, einen Chat, einen Prompt, ein Protokoll, einen Screenshot oder ein Dokument schreiben, und keinen KI-Agenten nach ihr suchen lassen.
- Nicht die Nummer eines anderen Produkts verwenden und die eigene nicht an einen Fork weitergeben.
- Code aus Pull Requests, Forks oder beliebigen Branches nicht mit dem Secret laufen lassen.
- Sie nicht in der Build-Ausgabe oder in Fehlermeldungen ausgeben und keine FinTS-Nachrichten protokollieren, die sie enthalten.
- Einen Build ohne Nummer nicht scheitern oder abstürzen lassen; er soll ohne direkten Bankzugang funktionieren und sagen, warum.

## Mehr dazu

- In diesem Repository: die Entscheidungen D-058, D-260 und D-745 in [`DECISIONS.md`](DECISIONS.md), die Bedrohung T-63 in [`THREAT_MODEL.md`](THREAT_MODEL.md), [`RELEASING.md`](RELEASING.md) und `crates/kassensturz-bank/build.rs`.
- Der öffentliche Quelltext und die Dokumentation von AqBanking, für die Laufzeit-Einstellung.
- Die FAQ der Registrierungsstelle auf fints.org.
