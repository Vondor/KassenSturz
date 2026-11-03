# Trying KassenSturz: the tester guide

*For testers. About 45 minutes. German original: [TESTER_GUIDE.de.md](TESTER_GUIDE.de.md); the German files are the ones handed to testers (testers are German-speaking, D-553).*

Thank you for trying KassenSturz, a free household-money program for your computer. It is meant to be simple enough that you need no accounting or economics background.

## What happens in the test

A session takes 45 to 60 minutes. The facilitator, the person who invited you, sits next to you or joins by video call. They hand you **task cards** one at a time. You solve them with KassenSturz while thinking aloud, then answer a few questions.

| When | What | About |
|---|---|---|
| Before | Read this guide, get your computer ready | 5 min |
| Test | Install (card 1) and cards 2 to 8 | 30 min |
| After | Closing questions | 10 min |

## 1. You can't do anything wrong

**We are testing the program, not you.** Where you get stuck, don't understand or get annoyed, that is exactly what we are looking for. A test where everything goes smoothly teaches us almost nothing.

- Nobody gets hurt by honest criticism. "That's silly" and "I don't understand this" are the most valuable sentences of the test.
- You can't break anything: you work with made-up numbers.
- No thought is too small. A quiet "hm" helps.
- Giving up is allowed. Say so and go to the next card.
- You can pause or stop at any time without giving a reason.

## 2. Thinking aloud

Say **everything that goes through your head** while you use the program, as if you were explaining it to a friend on the phone who can't see your screen. Example: "I'm looking for where to enter groceries … 'Add' at the top, let me try … hm, that's an account, not an expense … I don't know what 'part' means … ah, that worked, but I'm not sure it was saved."

**Why:** from outside we only see where you click. Why, what you expected and what confused you, we learn only if you say it.

- Keep talking, even if it feels odd at first. After two or three minutes it feels normal.
- Say what you expect before you click, and what really happens.
- Say how you feel: unsure, annoyed, surprised, relieved. Complaining is welcome.
- If you go quiet, the facilitator only asks: "What are you thinking now?" They won't help or explain; at home nobody sits next to you either.

## 3. What is safe

- **Made-up numbers and the example books only.** Don't type your own accounts, amounts, bank names or IBANs. If the program asks for a balance, invent one.
- **Nothing leaves your computer.** The test needs no bank, no AI service and, after installing, no internet. Leave AI and bank connections off.
- **If you do enter real data:** that is not a problem, please just tell us honestly (question 8 of the questionnaire). Your report is then treated as personal data: only the facilitator reads it, it is not published, not put into the project, the public bug list or any AI tool, and it is deleted after the round.
- **Recording only with your consent.** The facilitator asks first. "No" is a perfectly fine answer.
- **About developer mode.** For the test we start KassenSturz in *developer mode*, so it writes a log of where it was slow or failed. It contains only times, names of screens and buttons and error codes, **no amounts, names or text from your book**. If "Entwicklermodus" ("developer mode") shows in the window title, it is running. It is for testing, not everyday use. Keep AI off, or the AI conversation would be logged too.

## 4. What you need

- A computer with **Windows 11 (or 10)** or **Ubuntu**, and a step-by-step sheet for each: [Windows](INSTALL_WINDOWS.en.md), [Ubuntu](INSTALL_UBUNTU.en.md).
- Internet for the download (not afterwards); on Ubuntu, your computer password once.
- **Installing is part of the test.** The facilitator doesn't install for you and doesn't show you where to click. If you get stuck, say so: it's a valuable finding.

## 5. Installing in short

1. Open https://github.com/Vondor/KassenSturz/releases (no GitHub account needed). The top entry is the newest, marked *Pre-release*.
2. Under **Assets** download the file for your system: for Windows the file with "windows" in its name, for Ubuntu the file ending in `.flatpak`.
3. Windows warns twice because the test build isn't signed yet; the sheet shows how to continue. Ubuntu: set up Flatpak once (your password once), then the KDE runtime and the program, both for your user only.
4. Start in developer mode and create an account with an **invented** balance.

How to use KassenSturz day to day is in the [user guide](../user/USER_GUIDE.de.md) (German). You don't need it before the test.

## 6. The session, step by step

1. You read this guide.
2. The facilitator greets you, asks about recording and practises thinking aloud with you.
3. You answer "Before the test" ([questionnaire](QUESTIONNAIRE_1.en.md), section 1).
4. You get the task cards one at a time. Card 1 is installing, with the install sheet.
5. You answer "After the test".
6. For a problem you want to write down, use the [report page](REPORT_PAGE.en.md). It goes only to the facilitator.
