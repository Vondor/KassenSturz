# Datenschutzerklärung

*Gilt für KassenSturz, die freie Haushaltsbuch-App. Zuletzt gegen den Code geprüft: 17. November 2026. Englische Fassung: [PRIVACY.md](PRIVACY.md).*

## Kurz gesagt

- **Kein Konto, keine Telemetrie.** KassenSturz hat keine Anmeldung, keine Nutzungsstatistik, keine Absturzberichte, keine Werbung und keine Update-Abfrage. Das Projekt betreibt keinen Server, den Ihre Daten erreichen könnten.
- **Ihre Daten bleiben auf Ihrem Computer.** Ihr Haushaltsbuch ist eine Datei, deren Ort Sie wählen. Die Entwickler sehen sie nie.
- **Ohne Ihr Zutun wird nichts gesendet.** Nur zwei Funktionen können ins Internet oder zu einem anderen Computer verbinden, und beide sind aus, bis Sie sie benutzen: die Bankverbindung und die KI.

## Was KassenSturz speichert und wo

- Ihre Konten, Buchungen, Kategorien, Pläne und Einstellungen, in Ihrer Buchdatei und in Sicherungskopien neben dem Buch, auf Ihrem eigenen Computer.
- Auf Ihren Wunsch Ihre Bank-PIN im Passwortspeicher Ihres Systems (Windows-Anmeldeinformationsverwaltung, KWallet, GNOME-Schlüsselbund oder KeePassXC). Sie steht nie im Buch oder in einer Sicherung. Standardmäßig wird sie jedes Mal abgefragt oder bis zum Schließen der App im Arbeitsspeicher gehalten.

Nichts davon sendet KassenSturz an das Projekt, an SerpentSoftware oder an andere.

## Wann etwas Ihren Computer verlässt

1. **Bankverbindung (optional).** Richten Sie eine Bank ein, verbindet sich KassenSturz mit **Ihrer Bank** (FinTS/HBCI, verschlüsselt), um Kontostände und Umsätze zu lesen. Es sendet nie Zahlungen. Die Verbindung geht nur zu Ihrer Bank; deren Datenschutzerklärung gilt dort.
2. **KI (optional, standardmäßig aus).** Schalten Sie die KI ein und tragen einen Anbieter ein, sendet KassenSturz die Texte, die die KI für ihre Antwort braucht (zum Beispiel Zahlungsempfänger, Notizen oder Beträge, nach denen Sie fragen), **nur an den von Ihnen gewählten Anbieter**, und erst nachdem Sie einmal zugestimmt haben, mit einer Angabe, was gesendet wird. Wählen Sie ein Modell auf Ihrem eigenen Computer, verlässt nichts den Computer; ein Modell auf einem anderen Computer zählt als Verlassen und fragt nach Ihrer Zustimmung. Solange die KI aus ist, läuft kein KI-Code und nichts KI-Bezogenes öffnet eine Verbindung.

Die eigenen Bildschirme der App können nichts aus dem Internet laden. Einen Link zu öffnen (zum Beispiel die Projektseite) übernimmt Ihr Browser, weil Sie ihn angeklickt haben.

## Was wir nicht tun

Wir erheben, verkaufen oder teilen keine personenbezogenen Daten, erstellen keine Profile und nutzen Ihre Daten nicht für Werbung. Wir fragen nie nach Ihrer PIN oder einer TAN, und niemand aus dem Projekt wird das tun.

## Ihre Kontrolle

Löschen Sie die Buchdatei und ihre Sicherungen, bleibt nichts zurück. Sie können Ihre Daten jederzeit in einem offenen Format exportieren. Schalten Sie die KI in den Einstellungen aus, verschwindet sie aus der App. Richten Sie keine Bank ein, verbindet sich KassenSturz nie mit einer.

## Andere Stellen

Melden Sie ein Problem auf GitHub oder beziehen Sie die App aus einem Store, gilt dafür die Datenschutzerklärung des jeweiligen Dienstes (zum Beispiel GitHub, Microsoft oder Canonical). Schreiben Sie keine echten Finanzdaten in eine Meldung.

## Änderungen und Kontakt

Ändert sich etwas, ändert sich zuerst diese Seite, in der öffentlichen Projekthistorie. Fragen: Eröffnen Sie ein Issue unter <https://github.com/Vondor/KassenSturz/issues>. Die Angaben sind gegen den Code und das [Bedrohungsmodell](THREAT_MODEL.md) geprüft; mehr Einzelheiten in [Wie sicher ist KassenSturz?](SECURITY_OVERVIEW.de.md).
