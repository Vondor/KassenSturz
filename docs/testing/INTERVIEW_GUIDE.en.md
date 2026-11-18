# KassenSturz: facilitator interview guide (Beta)

*Team copy of [INTERVIEW_GUIDE.de.md](INTERVIEW_GUIDE.de.md), the guide used in sessions (German testers, D-553). For the person who accompanies the session; belongs with the [tester guide](TESTER_GUIDE.en.md) and the [install step log](INSTALL_LOG.en.md). 30 to 60 minutes. Quoted blocks are **read aloud word for word** (the German text is the one read); everything else is for the facilitator.*

## 0. What the test is about (facilitator only)

We are **not looking for bugs.** We want to understand **how KassenSturz feels to people**: what they see first, what they imagine it to be, where they feel safe and where not, and which paths they take by themselves. The tester is guided through some features and we listen to how they experience them.

- **Not every feature is for everyone.** Someone who only wants to know where the money goes needs no formulas. Someone who works with tables daily may look for exactly those. Someone who programs wants to build things. So Part C has **three paths** (four in the long version) and the tester walks only one.
- **Giving up is not failure but information:** "This is not for me" is a finding about fit, not ability.
- **Crashes and errors** are noted, not hunted or probed. If something breaks, calmly carry on.
- What counts is **what the tester says and expects**, not only whether they reach the goal.

## 1. Preparation

- **Installation is done.** The tester installed at home alone with the sheet and sent the step log, which you read beforehand. If they were stuck there was exactly one hint by e-mail, noted in the log. **The facilitator never installs for the tester.**
- **Example book:** only **Suburban Family** (Vorstadtfamilie); no book of their own. It is open on the overview before the start.
- KassenSturz runs in **developer mode**; bank connections off; **AI off at the start**, switched on only on path 4, by the tester.
- **For path 4 (long version only):** an **access card** with the model's full address including `http://` and port, and an **own, time-limited key for this tester only**, blocked after the session. All testers get the **same model**. Check beforehand that it is reachable.
- **Recording** of screen and audio is prepared but **not started** (section 3). The tester shares their **whole screen** in a browser video call (nothing to install). Recording happens **only on the facilitator's computer**, never the tester's. If both sit in one room, **each gets their own microphone on its own audio track** (two tracks in OBS). Test once beforehand. Right after starting, say the time aloud ("It is now 18:32") so recording and developer-mode log can be matched later.
- The developer-mode **log** stays on the tester's computer; they send it after the session with their notes ([report page](REPORT_PAGE.en.md)).
- **Tidy the screen:** ask the tester to close other programs, e-mail, chat and notifications and to have no own files open.
- **Check the keyboard:** on a test computer or VM it is set to **German**; otherwise the tester hunts for characters and we measure the setup, not the program.
- Cards are ready **one at a time**; the tester sees only the current card, and from Part C only those of the chosen path.
- Notes (section 8) and a clock ready; quiet place, 60 minutes, something to drink.

### Schedule (minutes)

| Part | What | Short (30) | Long (60) |
|---|---|---|---|
| 2 | Greeting | 2 | 3 |
| 3 | Recording: consent | 2 | 2 |
| 4 | Practise thinking aloud, three questions | 3 | 5 |
| 5 | Part A: the first look | 3 | 5 |
| 6 | Part B: everyday | 6 | 12 |
| 6 | Part C: a path of choice | 8 | 20 |
| 7 | Closing talk | 6 | 13 |

Short version: in B only cards B1 and B2, in C only the first card of the path, in the closing talk only the questions marked ★.

## 2. Greeting (read aloud)

> Thank you for taking the time. My name is ____ and I accompany today's session.
>
> KassenSturz is a program for the household money, and it is still in progress. Today I mainly want to know one thing: **how does the program feel to you?** What you think when you see it, what you expect, what you like and what you don't. You don't have to look for bugs, and you can't do anything wrong. There are no right or wrong answers.
>
> I worked on the program. Please be completely open anyway, also if you dislike something. That helps me more than politeness.
>
> Today we work in an **example book** with an invented family and invented numbers. **Please enter no real data of your own**: no own accounts, amounts, banks or IBANs.
>
> You can pause or stop at any time without giving a reason.

## 3. May I record? (read aloud before anything is recorded)

**Recording is still off.** Ask first, then record.

> I would like to **record your screen and the sound**, what you say and what you do on the screen. Then I can listen to you now instead of taking notes, and look at it all later in peace.
>
> Only I see and hear the recording. It is not published or passed on and is deleted after 4 weeks at the latest. What I write down from it does not contain your name.
>
> **Saying no is completely fine.** The session then runs the same and I only take notes. You can also have the recording stopped at any time: just say "stop".
>
> May I start the recording?

- Switch on **only after a clear yes.** Hesitation, "well" or "does it have to be?" means **no**.
- After switching on, repeat on the recording: "The recording is running now. Do you agree?" and wait for the yes.
- On no: do not ask again or persuade. "No problem, then I take notes."
- If the tester later says "stop": stop at once and ask whether the part so far should be deleted.
- The recording counts as personal data: only with the facilitator, never in git, an issue, the project files or with an agent; deleted after 4 weeks at the latest. Before an agent gets anything from it, names, voices and anything that points to a person, a bank or a program are removed.
- Notes: recording ☐ yes ☐ no; consent repeated on the recording ☐ yes.

## 4. Practise thinking aloud, three questions

> I ask you for something unusual today: **to think aloud.** Please say everything that goes through your head: what you see, what you look for, what you expect, what surprises you. As if telling someone on the phone who can't see your screen. It feels odd at first; after a few minutes it comes by itself.
>
> I'll show it once, briefly.

**Demonstrate (about 30 seconds, not with KassenSturz):** set an alarm for 7 o'clock on a phone, thinking aloud ("I look for the clock … that's the timer, no … here 'alarm' … hopefully that's for the morning …").

> Now you, to practise: tell me aloud how many windows your home has, going through the rooms in your mind.

Afterwards: "Exactly like that, thank you."

> One more thing: **I won't help you during the tasks** and won't explain anything. That is not rudeness; I want to see how the program feels without help. I write down your questions and answer everything at the end. If you go quiet I only ask: "What are you thinking right now?"

**Three questions** (spoken, answers into the notes):

1. How do you keep track today of what your money is spent on? Just tell me briefly.
2. Do you otherwise work with spreadsheets, such as Excel or LibreOffice Calc? ☐ hardly ☐ sometimes ☐ often, also with formulas
3. Do you program, at work or as a hobby? ☐ no ☐ a little ☐ yes

Answers 2 and 3 help later with Part C. They are **not** read out or commented on.

## 5. Part A: the first look

The overview of Suburban Family is open; nothing clicked yet.

> This is KassenSturz with the example book of an invented family. Please don't click anything yet. Look at the page in peace and tell me what you see.

Then one at a time, with pauses:

> What do you think this program is for? And for whom?
>
> How is the family doing with its money, as you see it here?
>
> If you were allowed to click something now: what first, and what do you expect then?

**Observe:** which parts of the page are named, which overlooked? Which words does the tester pick up, which do they paraphrase? How do they talk about the family (judging, describing, comparing themselves)?

## 6. The cards

Give each card **individually**:

> Here is the next card. Please read it aloud, and tell me when you think you are done or if you don't want to continue.

**After every card** exactly these two questions, always the same:

> How was that for you?
>
> What did you expect, and what happened then?

No rating scale after each card; the closing talk collects judgements. (Print cards: [Aufgabenkarten-Druck.pdf](Aufgabenkarten-Druck.pdf), German.)

### Part B: everyday (all testers)

**Card B1: Where did the money go?** — The family wonders what most money was spent on last month. Find out.

**Card B2: A purchase** — Yesterday someone in the family bought groceries for €23.80 at the supermarket. Enter that.

**Card B3: Back again** (long only) — You changed your mind: the purchase should not be in the book after all. Make that so.

**Card B4: Does it come back?** (long only) — The family pays rent every month. Make sure the program counts on it, also for the coming months.

### Part C: a path of choice

Read the paths aloud (or hand over the "Three ways" card) and **let the tester choose**:

> Now it is about analyses: getting numbers and charts about the family's money. There are three ways, in the long version four. Please choose the one that suits you best. No way is better or worse.
>
> **Path 1: Click it together.** You are guided step by step and choose what you want to see.
>
> **Path 2: Calculate like in a spreadsheet.** You write what is to be calculated yourself, like formulas in Excel.
>
> **Path 3: Build it yourself.** You describe a chart or a search rule in a small language, like programming.
>
> **Path 4: Ask questions.** You switch on an AI and ask it in your own words. *(long version only)*

- **Do not suggest before the tester has chosen.** If asked "What should I take?", answer: "What would you take at home?"
- Note: chosen path, the reason in their words, and whether the choice fits questions 2 and 3. A choice that does not fit is **especially informative**; do not redirect.
- **A second path (long version, if time remains):** "Would you like to try one of the other paths?" Offer only, never push, never suggest one. The tester chooses again; any combination is possible, also path 4 second. At most two paths (about 10 minutes each). Note the **order**: someone who found an answer first and then asks the AI compares differently from someone who starts with the AI; that is intended and taken up in the closing talk.

**Path 1, C1-1: A picture of the money** — Build a chart that shows what the family spent its money on in the last three months. If you like it, put it on the start page.

**Path 1, C1-2: Compare** (long only) — The family wants to know whether it spends more on groceries this year than last year. Show it the way you would like to see it yourself.

**Path 2, C2-1: A share** — The family wants to see one number on its start page: what percentage of last month's spending was groceries? Set that up.

**Path 2, C2-2: What is left** (long only) — Set up a number showing how much of the salary is left once the rent is paid. It should always apply to the month the page currently shows.

**Path 3, C3-1: A chart of your own** — Create a chart on a page whose look you describe yourself. Change something you are interested in, for example colour, type or what is shown.

**Path 3, C3-2: A search rule** (long only) — The program should assign bookings to a category by itself when their text follows a certain form, for example every booking whose text contains an invoice number. Set up something like that.

**Path 4, C4-1: Switch the AI on** — You want to ask the program questions in your own words. For that there is an AI that has to be switched on first. This card carries the address and your access. Set it up.

Hand over the access card. The facilitator does **not** set it up and types nothing. The program's consent request is answered by the tester themselves, so it is on the recording. If the tester is stuck for more than 5 minutes, the rule "When the tester is stuck" applies; the setup time is noted separately from C4-2.

**At the consent pages the facilitator is silent**, even if asked ("Do I have to agree?"): only "What would you do at home?". If the tester does not agree, that is a valid result; the session goes on without AI (notes: "AI declined").

**If the tester can't find the chat after switching on:** after about 3 minutes the facilitator only says **where** it is and gives **no question**. Note with time: "Chat shown".

**Path 4, C4-2: Ask** — Ask the AI what the family spent the most on last month. Then ask something you yourself want to know about the family's money.

**Observe on path 4:** how is the consent read (read, skimmed, clicked away)? Is the AI's answer compared with the numbers on the page? How does the tester handle a suggestion they must confirm first? Is the AI addressed like a tool or like a counterpart?

**After path 4:** the tester switches the AI off in the settings. The facilitator blocks the key after the session.

**At the end of each path** (read aloud):
> If you could wish for something that doesn't exist yet: what would you build or want to see?

### What the facilitator may say

- "What are you thinking right now?" / "What are you looking for?" / "What do you expect to happen now?"
- "Tell me more." / "Mhm."
- To a question from the tester: "What would you guess?" or "What would you do at home now?"

### What the facilitator does not do

- not help, not show, not point at the screen, not explain words from the program;
- not judge how the tester solves something ("good", "almost"); allowed only: "Thank you, that helps a lot.";
- no leading questions ("Was that hard?", "Do you like it?"; better: "How was that?");
- not name buttons or pages the tester has not named themselves;
- not sigh, laugh or nod.

### When the tester is stuck

After about 3 minutes without progress or visible frustration:
> Thank you, that is an informative spot. Would you like to try a little longer, or shall we go to the next card?

Only then may the facilitator show the way, if the next card builds on it. Note: "Help given at …".

**If the problem is our setup, not the program** (wrong keyboard, a card error, the video call, the facilitator's computer), the facilitator may fix it at once. Note with time: "Technical help: …". This time does not count as time with the program.

**If the tester types real data:** do not interrupt or scold. Note: "Real data: yes". Ask at the end (question 9).

**If the program crashes:** note the time.
> That is the program's fault, not yours. We restart it and carry on.

## 7. Closing talk (read aloud)

> Those were the cards, thank you. Now I would like to ask you a few more things. Afterwards you may ask me anything.

Ask openly, in this order, and leave time. Probe only with "Tell me more" or "What makes you say that?". Questions marked ★ are also in the short version.

1. ★ If you had to tell someone in one sentence what KassenSturz is: what would you say?
2. ★ How did the program come across to you? Is there a word that describes it?
3. Was there a moment you felt safe and one you felt unsure? Tell me about both. And how did you experience yourself using it today? Is that similar or different with other programs?
4. ★ Today's numbers were invented. If they were your own: how much would you trust such numbers? What makes you say that?
5. Was there a word or text that seemed strange, or where you would have expected something else?
6. ★ Part C (name the chosen path from the notes, e.g. "You chose path 1"): why did you choose that path? In hindsight, would you rather have taken another? (With two paths: how was the second compared with the first?)
7. Who do you think this program is made for? Are you one of them?
8. ★ How do you keep your own money matters on the computer safe from others today, and what would you expect from such a program? What would it have to offer for you to entrust it with your money matters?
9. One last question, and every answer is fine: did you enter any real data of your own anywhere today? I only ask so that we protect your recording and notes accordingly.

Then answer the tester's questions and show what they were looking for. At the end:
> You helped us a lot today. Thank you.

**After the session:** stop the recording and store it safely (only with the facilitator). Have KassenSturz restarted without developer mode or uninstalled. File the notes with the code only (T1, T2, …).

## 8. Facilitator notes

Head: date · code (no name) · computer · version number · short/long · recording yes/no · real data yes/no · on path 4: model, setup time, key blocked ☐

Questions: 1 ____ · 2 ☐ hardly ☐ sometimes ☐ often · 3 ☐ no ☐ a little ☐ yes
Part C: first path ☐ 1 ☐ 2 ☐ 3 ☐ 4 · second path ☐ none ☐ 1 ☐ 2 ☐ 3 ☐ 4 · reason (verbatim) ____ · fits questions ☐ yes ☐ no

| Time (recording) | Part / card | What the tester said or did (verbatim if possible) | Expectation ↔ what happened | Help given? |
|---|---|---|---|---|

**Observe instead of judge:** note what was said and done, not whether it was "good". Especially: places where speaking and doing diverge (says "oh, easy" and then searches long), and places where the tester compares themselves with others ("this is probably more for …").

The three most striking moments, noted right after: 1. ____ 2. ____ 3. ____

With real data: notes and recording only with the facilitator, never in git, the bug list or with an agent; deleted after the evaluation.
