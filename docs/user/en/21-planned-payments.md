# 21. Planned payments

*A planned payment is a payment you expect again and again: the rent, an insurance, your salary. KassenSturz matches your bank's entries to it and shows on the overview how much is really left this month. A plan is only a plan: it moves no money and sends nothing.*

## Planning a payment

- **The quickest way:** open an entry in an account, for example last month's rent, and choose **Plan this again…**. The form is filled in from the entry; check it and save. The entry becomes the first payment of the plan.
- **Or by hand:** choose **Plans** in the side menu, right under **Budgets**, then **Add a plan**. **Accounts › Planned payments…** opens the same list as a shortcut.

The form asks for:

- **What is it for:** a name such as "Rent".
- The account, **Goes out** or **Comes in**, and the amount.
- A category (optional): an entry that matches the plan and has no category yet gets this one. A Rule's category or your own always stays.
- **How often:** every week, every month, every year, or every few of them, and **First due on**. A payment that falls on a Saturday or Sunday is expected on the Monday after. A line under the form shows the next three days in words.
- **Ends:** never, or on a day.
- **The amount may vary by up to** (optional): for a bill that is a little different every time, such as electricity. An entry within that much still counts as the payment.
- **Paid to** or **Received from** (optional): the name or IBAN the bank shows for the other side. With it, KassenSturz only takes entries of that person or company, and it can notice when their amount changes (see below).

## When your bank's entries come in

After you read in a statement or fetch from your bank, every new entry that fits a plan's payment is *matched* to it: the same account, the money going the same way, the plan's amount (within what it may vary by) and at most three banking days from the day. The summary lists them under **Planned payments found**. **Undo these matches** takes them back; the entries stay. Nothing is booked and no amount or date is changed.

## "Did this happen?"

When a payment's day has passed by more than three banking days and no entry fits, **Planned payments…** shows it under **Did this happen?**:

- **Match an entry…**: you pick the entry yourself, whatever its amount.
- **Book it…**: for an account you keep by hand. It adds the entry with the amount and day you leave in the form.
- **Skip this time**: the payment is not expected any more this time.

Nothing is matched, booked or skipped without your click, and **Undo** takes each step back.

## When the amount changes

Salaries rise, rents go up, a contract gets cheaper. KassenSturz notices it and asks; it never changes a plan by itself.

- **After reading in a statement:** when a new entry names the plan's person or company (its **Paid to** or **Received from**) at about the right day but with another amount, it is *not* matched. The summary says **1 planned payment may have a new amount.** with the entry's and the plan's amount.
- **Under "Did this happen?"** the payment then says "An entry of … on … names the same person or company. It may be this payment with a new amount." **It's this one, …** matches that entry. You can also pick it with **Match an entry…**.
- **The question:** as soon as an entry with another amount is matched, by either button, or booked with another amount with **Book it…**, KassenSturz asks **Expect … from now on?**
  - **Expect …** changes the plan's amount to the entry's. Payments already matched keep their own entries' amounts, so last month's figures don't change. It is one change in the history, and **Undo** takes it back.
  - **Keep …** leaves the plan as it is. Nothing is stored, so the next entry with another amount asks again.
- A plan without **Paid to** or **Received from** can't tell a changed payment from any other entry, so it doesn't point one out. Match the entry yourself with **Match an entry…**; the question comes all the same.
- A bill that is a little different every time is better planned with **The amount may vary by up to**: then each entry within it is simply matched and nothing is asked.

You can also change the amount any time with **Edit** in the list of planned payments.

## Changing, pausing, removing

In **Your planned payments** each plan has **Edit**, **Pause** (**Go on** lets it continue) and **Remove**. A paused plan is not expected, matched or counted. Removing a plan keeps the entries it matched; KassenSturz just stops expecting the payment, and **Undo** brings the plan back. Closing an account pauses its plans until you open it again.

## On the overview

- **Left this month** is your balance now minus the planned payments still to go out this month. Planned income that hasn't arrived is not added; a second line says what the month ends with if it comes. Spending that is not planned is not included.
- **Coming up** lists the planned payments of the next 30 days.
- **Month by month** in **Planned payments…** shows the next months: start, planned in, planned out and end.

If your overview doesn't show the two tiles, add them with **Edit layout** and **Add to the page** (page 4).

**If something looks different:** an entry that you expected to be matched but wasn't may be outside the three banking days, in another account, or of another amount; then use **Match an entry…**. A payment of a paused plan or a closed account is not shown.
