# Trying KassenSturz: the tester guide

*For testers. About 45 minutes. German original: [TESTER_GUIDE.de.md](TESTER_GUIDE.de.md); the German files are the ones handed to testers (testers are German-speaking, D-553).*

Thank you for trying KassenSturz, a free household-money program for your computer. It is meant to be simple enough that you need no accounting or economics background.

## What happens in the test

Before the session you install KassenSturz at home, alone, with the install sheet and the [step log](INSTALL_LOG.en.md). The session itself takes 45 to 60 minutes. The facilitator, the person who invited you, sits next to you or joins by video call. They hand you **task cards** one at a time. You solve them with KassenSturz while thinking aloud, then answer a few questions.

| When | What | About |
|---|---|---|
| Before, at home | Read this guide, install KassenSturz (card 1), send the step log | about 20 min |
| Session | Cards 2 to 5, 7 and 8 | 30 min |
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

> **Please, no real data.** Use only the example book in the test. If you still enter your own accounts, amounts, banks or IBANs, that data is recorded and analysed: in the screen and audio recording, in the app's log and in your report. That is why we ask you not to.

- **Made-up numbers and the example books only.** Don't type your own accounts, amounts, bank names or IBANs. If the program asks for a balance, invent one.
- **Nothing leaves your computer.** The test needs no bank, no AI service and, after installing, no internet. Leave AI and bank connections off.
- **If you do enter real data:** that is not a problem, please just tell us honestly (question 8 of the questionnaire). Your report is then treated as personal data: only the facilitator reads it, it is not published, not put into the project, the public bug list or any AI tool, and it is deleted after the round.
- **Your household book is not encrypted.** KassenSturz stores your household book unencrypted on your computer. Anyone who can use your computer can open it. So protect your user account with a password and, if you can, switch on your system's encryption (on Windows "Device encryption" or BitLocker).
- **Recording only with your consent.** The facilitator asks first. "No" is a perfectly fine answer.
- **About developer mode.** For the test we start KassenSturz in *developer mode*, so it writes a log of where it was slow or failed. It contains only times, names of screens and buttons and error codes, **no amounts, names or text from your book**. If "Entwicklermodus" ("developer mode") shows in the window title, it is running. It is for testing, not everyday use. Keep AI off, or the AI conversation would be logged too.

## 4. What you need

- A computer with **Windows 11 (or 10)** or **Ubuntu**, and a step-by-step sheet for each: [Windows](INSTALL_WINDOWS.en.md), [Ubuntu](INSTALL_UBUNTU.en.md).
- Internet for the download (not afterwards); on Ubuntu, your computer password once.
- **Installing is part of the test.** You install at home, alone, with the install sheet only; nobody helps, not even by phone. Note each step in the [step log](INSTALL_LOG.en.md). If you get stuck, write down where, stop and send the log to the facilitator: it's a valuable finding, and you get a hint by e-mail.

## 5. Installing in short

1. Open https://github.com/Vondor/KassenSturz/releases (no GitHub account needed). The top entry is the newest Beta, marked *Pre-release*; the facilitator tells you its number (for example 0.261117.8).
2. Under **Assets** download the file for your system: for Windows the file ending in `-windows-x64-setup.exe`. On Ubuntu you usually need no file: you install KassenSturz with one command from the Snap Store.
3. Windows warns twice because the Beta isn't signed yet; the sheet shows how to continue. Ubuntu: one command in the terminal, with your password once; if that fails, the sheet shows a second way with a Flatpak file.
4. Start in developer mode and create an account with an **invented** balance.

How to use KassenSturz day to day is in the [user guide](../user/USER_GUIDE.de.md) (German). You don't need it before the test.

## 6. The session, step by step

1. You read this guide.
2. You install KassenSturz at home (card 1) and send the step log to the facilitator.
3. The facilitator greets you, asks about recording and practises thinking aloud with you.
4. You answer "Before the test" ([questionnaire](QUESTIONNAIRE_1.en.md), section 1).
5. You get the task cards from card 2 on, one at a time.
6. You answer "After the test".
7. For a problem you want to write down, use the [report page](REPORT_PAGE.en.md). It goes only to the facilitator.
