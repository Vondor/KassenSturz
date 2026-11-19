<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# KassenSturz auf macOS: der experimentelle Download

**Stand: experimentell und ungetestet (D-875).** Niemand im Team besitzt einen Mac oder eine Mac-virtuelle-Maschine, deshalb hat noch nie ein Mensch diesen Build gestartet. Die einzige Prüfung ist ein automatischer Start der App auf einem macOS-Rechner, den GitHub bereitstellt (`.github/workflows/macos-app.yml`): Die App öffnet ihren Begrüßungsbildschirm ohne Fenster und läuft 30 Sekunden weiter. Rechnen Sie mit Fehlern. Bitte nur ein Beispielbuch benutzen, nie Ihre eigenen Daten, und melden Sie, was Sie finden, als Issue.

## Was es ist

Zu jedem getaggten Vorabversions-Release (`v0.*`, siehe `docs/RELEASING.md`) kommt eine Datei `KassenSturz-<Version>-macos-arm64-experimental.dmg` auf die Releases-Seite, kurz nach dem Release selbst. Ist der Mac-Build fehlgeschlagen, fehlt die Datei; das Release bleibt davon unberührt.

- **Nur Apple Silicon** (M1 oder neuer). Einen Intel-Build gibt es nicht.
- **Nicht von Apple signiert und nicht notarisiert.** Dafür braucht es einen Apple-Developer-Account (99 USD im Jahr), den das Projekt nicht hat (D-048). Die App trägt nur eine Ad-hoc-Signatur, die Apple Silicon zum Starten überhaupt verlangt; sie sorgt nicht dafür, dass macOS der App vertraut.
- **Keine Bankverbindungen.** AqBanking ist nicht Teil dieses Builds, wie bei der Windows-App (D-054).
- Gespeicherte Bank-PINs würden im macOS-Schlüsselbund landen, das wurde aber nie ausprobiert.
- Der Mac App Store und Homebrew verlangen beide eine signierte App, deshalb gibt es keinen Store für diesen Build.

## So öffnen Sie ihn

1. Öffnen Sie die `.dmg` und ziehen Sie **KassenSturz** auf **Programme**.
2. Starten Sie KassenSturz. macOS meldet, dass es die App nicht prüfen kann, und öffnet sie nicht. Klicken Sie auf **Fertig**.
3. Öffnen Sie **Systemeinstellungen > Datenschutz & Sicherheit**.
4. Scrollen Sie nach unten zur Meldung, dass KassenSturz blockiert wurde, und klicken Sie auf **Trotzdem öffnen**.
5. Geben Sie Ihr Mac-Passwort ein und bestätigen Sie mit **Öffnen**. Das ist nur beim ersten Mal nötig.

Im Terminal geht es auch: `xattr -dr com.apple.quarantine /Applications/KassenSturz.app`.

Derselbe Text, deutsch und englisch, liegt in der `.dmg` als `SO OEFFNEN - HOW TO OPEN.txt`.

## Warum es noch keine Bankverbindung gibt (D-876)

Der Mac-Build lässt FinTS-Bankverbindungen mit Absicht weg, bis jemand mit einem Mac sie testen kann:

- Bankverbindungen brauchen AqBanking und dessen Hilfsbibliothek Gwenhywfar. KDE Craft, das den Rest der Mac-App baut, hat dafür unter macOS kein Paket; sie müssten getrennt gebaut und von Hand in die App gepackt werden.
- Niemand im Team hat einen Mac. Eine Bank-Anmeldung mit echter PIN und echten Konten ließe sich also nicht ein einziges Mal ausprobieren, bevor ein Tester sie benutzt. Das Geld geht vor (AGENTS.md Abschnitt 2): ein ungetesteter Bankweg ist schlechter als keiner.
- Die offizielle FinTS-Registrierungsnummer steckt nur in Release-Builds mit funktionierendem Bankweg; in einen Build, den niemand ausprobieren kann, einzubauen, würde ihn nicht zum Laufen bringen.

Alles andere geht ohne Bank: Sie können Kontoauszüge aus Dateien einlesen (CAMT, MT940, OFX, CSV). Bankverbindungen für den Mac folgen, sobald es einen Mac-Tester gibt.
