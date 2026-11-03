# KassenSturz facilitator script (round 1)

*Team copy of [FACILITATOR_SCRIPT.de.md](FACILITATOR_SCRIPT.de.md), the one that is read aloud to German-speaking testers. In every round the facilitator is the maintainer (D-552). About 60 minutes with before and after.*

Text marked "say" is read **word for word** in the German file; here its content is summarised.

1. **Preparation.** Nothing installed and no installer handed over: card 1 measures installation. The facilitator **never installs for the tester**. Example books ship with the app; the matching statement file (card 5) and the newer build (card 6) are ready. AI and bank connections off. Check that the statements match the book: the repository's statements and answer sheets belong to reference month 2026-11; the app shifts a new example book's dates to the running month and holds 24 months (D-584), so before any later round rerun `tools/example-books/generate.py --reference-month YYYY-MM`, let the `example_books` test rewrite the answer sheets, and play cards 4 and 5 through once yourself. Cards handed out one at a time. Notes template ready. Recording stays **off**. Agree privately how the tester will send their questionnaire.
2. **Greeting (say).** We test the program, not you. The facilitator helped build it, so blunt criticism is a favour, not an insult. Made-up numbers and example books only; nothing leaves the computer. Pause or stop any time.
3. **Recording consent (2a, say).** Ask before recording, never the reverse. Only the facilitator hears and sees it, nothing is published, it is deleted after evaluation (at the latest after __ weeks), no name on it. "No" is fine and changes nothing. Record only on a clear yes; hesitation means no. Repeat the question on the recording. A later "stop" ends it at once. A recording counts like a report with personal data: facilitator only, never git, issue, shared files or an agent.
4. **Think aloud (say, practise).** Demonstrate on an everyday task (setting an alarm); let the tester practise (counting the windows of their home aloud). "I won't help you during the test, this includes installing. If you go quiet I only ask: what are you thinking now?"
5. **During the test.** Questionnaire section 1 first, then one card at a time ("read it aloud, tell me when you're done"). After each card: where did you get stuck, ease 1 to 5; time card 1.
   - May say: "What are you thinking now?", "What are you looking for?", "What did you expect?", "Tell me more", "Mhm". To a question: "What would you guess?" or "What would you do at home?"
   - Never: help, point, explain program words, praise or scold solutions (only thank them for thinking aloud), ask leading questions, sigh or nod.
   - Note every hesitation, "hm", backtrack and search with time and card.
   - Stuck about 3 minutes or visibly frustrated: offer to keep trying or move on; show the way only if the next card depends on it and note "help given". On card 1 only say which step of the sheet they are at, never where to click.
   - Real data typed: don't interrupt; note it; ask question 8 at the end. Crash: note the time, stay calm, restart.
6. **Closing (say).** "After the test" questions, question 8 last and gently. Answer the tester's questions. Explain where the questionnaire and reports go. Afterwards restart KassenSturz without developer mode (or uninstall). Keep notes under a short code, no names.

## Success criteria (facilitator only)

Per card: **done** (no help, right result, in target time), **with effort** (over time, detours, or a self-fixed error), **not done** (gave up, needed help, or a wrong result the tester believes: the worst case).

| Card | Done when | Target |
|---|---|---|
| 1 | overview with own account, developer mode in title, by the sheet alone | 10 min |
| 2 | three entries right in amount, date, direction; categories fit; balance = start − 42.50 − 3.20 + 1,800 | 5 min |
| 3 | coffee 2.30; 200 € booked as a transfer (not spending); last step undone | 7 min |
| 4 | food total and in-vs-out match the book's answer sheet | 3 min |
| 5 | statement in the right account, duplicate not taken twice, preview read; new lines and closing balance match the answer sheet | 7 min |
| 6 | newer build installed, book opens, card 2 entries present | 5 min |
| 7 | every finding counts | 3 min |
| 8 | a rule sorts coffee; a new chart is on the overview | 7 min |

Answer sheets come from the real import and report code for each example book (step M7a.8), never by hand; facilitator only.

**Card 5 note (Suburban Family):** if the tester's date is before 6 Nov 2026, the checking account shows "Darin Buchungen nach heute: −201,15 €" after the CAMT import. This is expected: some lines in the file are dated in the future for the tester's computer. Say nothing in advance; if asked, use the neutral prompt. The statement's closing balance stays the comparison value. It disappears from 6 Nov 2026.

**A round passes** when, over all testers: no wrong result is believed (cards 2 to 5); cards 1 to 6 are done by at least 4 of 5 and none by more than one only "with effort"; average ease at least 4, no card under 3; nobody answers "no" to trust (3.1); at least half "yes" or "maybe" to continue (3.6); no crash or data loss. Any spot where two or more testers stall is a finding. With a single tester (the maintainer), 1, 4 and 6 count fully.

## Notes table

Time | card | what the tester said or did | stuck? | help given? | result. At the end: date, facilitator, tester code (no name), computer, version, recording yes/no, real data yes/no. Three most striking moments straight afterwards. Notes with real data stay with the facilitator and are deleted after evaluation.
