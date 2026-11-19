# 21. Geplante Zahlungen

*Eine geplante Zahlung ist eine Zahlung, die Sie immer wieder erwarten: die Miete, eine Versicherung, Ihr Gehalt. KassenSturz ordnet ihr die Buchungen Ihrer Bank zu und zeigt auf der Übersicht, wie viel in diesem Monat wirklich übrig ist. Ein Plan ist nur ein Plan: Er bewegt kein Geld und schickt nichts ab.*

## Eine Zahlung planen

- **Am schnellsten:** Öffnen Sie in einem Konto eine Buchung, zum Beispiel die Miete vom letzten Monat, und wählen Sie **Wieder einplanen…**. Das Formular ist aus der Buchung ausgefüllt; prüfen Sie es und speichern Sie. Die Buchung wird die erste Zahlung des Plans.
- **Oder von Hand:** Wählen Sie im Seitenmenü **Pläne**, direkt unter **Budgets**, dann **Plan hinzufügen**. **Konten › Geplante Zahlungen…** öffnet dieselbe Liste als Abkürzung.

Das Formular fragt nach:

- **Wofür ist sie:** ein Name wie „Miete“.
- Dem Konto, **Geht raus** oder **Kommt rein**, und dem Betrag.
- Einer Kategorie (freiwillig): Eine Buchung, die zum Plan passt und noch keine Kategorie hat, bekommt diese. Die Kategorie einer Regel oder Ihre eigene bleibt immer.
- **Wie oft:** jede Woche, jeden Monat, jedes Jahr oder alle paar davon, und **Zuerst fällig am**. Eine Zahlung an einem Samstag oder Sonntag wird am Montag danach erwartet. Eine Zeile unter dem Formular zeigt die nächsten drei Tage in Worten.
- **Endet:** nie oder an einem Tag.
- **Der Betrag darf schwanken um bis zu** (freiwillig): für eine Rechnung, die jedes Mal etwas anders ist, etwa Strom. Eine Buchung innerhalb dieser Spanne zählt trotzdem als die Zahlung.
- **Gezahlt an** oder **Erhalten von** (freiwillig): der Name oder die IBAN, die die Bank für die andere Seite zeigt. Damit nimmt KassenSturz nur Buchungen dieser Person oder Firma, und es kann bemerken, wenn sich ihr Betrag ändert (siehe unten).

## Wenn die Buchungen Ihrer Bank kommen

Nachdem Sie einen Kontoauszug eingelesen oder bei der Bank abgerufen haben, wird jede neue Buchung, die zu einer Zahlung eines Plans passt, ihr *zugeordnet*: dasselbe Konto, das Geld geht in dieselbe Richtung, der Betrag des Plans (innerhalb der erlaubten Schwankung) und höchstens drei Bankarbeitstage vom Tag entfernt. Die Zusammenfassung listet sie unter **Geplante Zahlungen gefunden**. **Diese Zuordnungen rückgängig machen** nimmt sie zurück; die Buchungen bleiben. Nichts wird gebucht, und kein Betrag und kein Datum wird geändert.

## „Ist das passiert?“

Wenn der Tag einer Zahlung mehr als drei Bankarbeitstage vorbei ist und keine Buchung passt, zeigt **Geplante Zahlungen…** sie unter **Ist das passiert?**:

- **Eine Buchung zuordnen…**: Sie wählen die Buchung selbst, egal mit welchem Betrag.
- **Buchen…**: für ein Konto, das Sie von Hand führen. Es trägt die Buchung mit dem Betrag und Tag ein, die Sie im Formular stehen lassen.
- **Dieses Mal überspringen**: Die Zahlung wird diesmal nicht mehr erwartet.

Nichts wird ohne Ihren Klick zugeordnet, gebucht oder übersprungen, und **Rückgängig** nimmt jeden Schritt zurück.

## Wenn sich der Betrag ändert

Gehälter steigen, Mieten gehen hoch, ein Vertrag wird billiger. KassenSturz bemerkt es und fragt; es ändert einen Plan nie von selbst.

- **Nach dem Einlesen eines Kontoauszugs:** Wenn eine neue Buchung die Person oder Firma des Plans nennt (sein **Gezahlt an** oder **Erhalten von**), etwa am richtigen Tag, aber mit einem anderen Betrag, wird sie *nicht* zugeordnet. Die Zusammenfassung sagt **1 geplante Zahlung hat vielleicht einen neuen Betrag.**, mit dem Betrag der Buchung und dem des Plans.
- **Unter „Ist das passiert?“** steht bei der Zahlung dann „Eine Buchung über … vom … nennt dieselbe Person oder Firma. Vielleicht ist es diese Zahlung mit einem neuen Betrag.“ **Das ist sie, …** ordnet diese Buchung zu. Sie können sie auch mit **Eine Buchung zuordnen…** wählen.
- **Die Frage:** Sobald eine Buchung mit anderem Betrag zugeordnet ist, mit einem der beiden Knöpfe, oder mit **Buchen…** mit anderem Betrag gebucht, fragt KassenSturz **Ab jetzt … erwarten?**
  - **… erwarten** ändert den Betrag des Plans auf den der Buchung. Schon zugeordnete Zahlungen behalten die Beträge ihrer eigenen Buchungen, die Zahlen vom letzten Monat ändern sich also nicht. Es ist eine Änderung im Verlauf, und **Rückgängig** nimmt sie zurück.
  - **… behalten** lässt den Plan, wie er ist. Nichts wird gespeichert; die nächste Buchung mit anderem Betrag fragt also wieder.
- Ein Plan ohne **Gezahlt an** oder **Erhalten von** kann eine geänderte Zahlung nicht von einer anderen Buchung unterscheiden und weist deshalb auf keine hin. Ordnen Sie die Buchung selbst mit **Eine Buchung zuordnen…** zu; die Frage kommt trotzdem.
- Eine Rechnung, die jedes Mal etwas anders ist, planen Sie besser mit **Der Betrag darf schwanken um bis zu**: Dann wird jede Buchung innerhalb dieser Spanne einfach zugeordnet, und es wird nichts gefragt.

Den Betrag können Sie auch jederzeit mit **Bearbeiten** in der Liste der geplanten Zahlungen ändern.

## Ändern, pausieren, entfernen

Unter **Ihre geplanten Zahlungen** hat jeder Plan **Bearbeiten**, **Pausieren** (**Fortsetzen** lässt ihn weiterlaufen) und **Entfernen**. Ein pausierter Plan wird nicht erwartet, nicht zugeordnet und nicht gezählt. Wenn Sie einen Plan entfernen, bleiben die Buchungen, die ihm zugeordnet waren; KassenSturz erwartet die Zahlung nur nicht mehr, und **Rückgängig** holt den Plan zurück. Wird ein Konto geschlossen, pausieren seine Pläne, bis Sie es wieder öffnen.

## Auf der Übersicht

- **Übrig in diesem Monat** ist Ihr Kontostand jetzt minus die geplanten Zahlungen, die in diesem Monat noch abgehen. Geplantes Geld, das noch nicht eingegangen ist, wird nicht dazugezählt; eine zweite Zeile sagt, womit der Monat endet, wenn es kommt. Ausgaben, die nicht geplant sind, sind nicht enthalten.
- **Demnächst** listet die geplanten Zahlungen der nächsten 30 Tage.
- **Monat für Monat** in **Geplante Zahlungen…** zeigt die nächsten Monate: Anfang, geplant rein, geplant raus und Ende.

Zeigt Ihre Übersicht die beiden Kacheln nicht, fügen Sie sie mit **Anordnung bearbeiten** und **Zur Seite hinzufügen** hinzu (Seite 4).

**Wenn etwas anders aussieht:** Eine Buchung, die Sie zugeordnet erwartet hätten, liegt vielleicht außerhalb der drei Bankarbeitstage, in einem anderen Konto oder hat einen anderen Betrag; dann nehmen Sie **Eine Buchung zuordnen…**. Eine Zahlung eines pausierten Plans oder eines geschlossenen Kontos wird nicht gezeigt.
