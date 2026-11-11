# 11. Connecting to your bank

*KassenSturz can fetch new transactions by itself from many German banks. This page shows how to set up a bank login and get new transactions. KassenSturz only reads; it can never send money.*

> **State of the test version:** the bank connection is still being checked. Depending on the version, it may be missing from the setup assistant; it is always in **Settings › Bank connections**. If you'd rather be on the safe side, read in files (page 10).

## What works and what doesn't

- KassenSturz fetches the balance and the booked transactions of checking accounts, savings accounts and credit cards. Securities accounts are listed as "not supported yet".
- It uses **FinTS**, the standard of the German banks. For accounts abroad, read in files.
- There is **no** function that starts transfers, standing orders or direct debits.
- The connection runs through **AqBanking**, a free program. The Flatpak and the Windows version bring it with them.

## Setting up a bank login

1. **Settings › Bank connections › Connect a bank…**
2. Type the **IBAN** of one of your accounts at this bank. KassenSturz finds the bank and asks only for what it needs: usually your **Login name**, where the bank has one a **Customer ID**, and your **PIN**.
3. Under **Remember the PIN**, choose how long KassenSturz keeps it (below).
4. If the bank asks for a **TAN** or for your **approval in its app**, KassenSturz shows the bank's question with the bank's buttons in a window of its own. A TAN goes to the bank once and is never saved. If you close the window, you cancel, and nothing half-finished is left behind.
5. At the end, KassenSturz lists the accounts your bank shows and suggests which account in your book each one fills. You confirm or change each choice.

If KassenSturz doesn't know your bank or its address, you can type the **FinTS address** yourself; it must start with `https://`. For co-operative banks missing from the lists, KassenSturz fills in the address their neighbours use and says that it is a guess. Check it if the connection fails.

KassenSturz can't yet show a TAN that the bank sends as a picture (a flicker code, a photo TAN or a QR code). It says so before it sends anything. At your bank, choose a TAN method that comes as text or that you approve in the bank's app.

## Where does your PIN go?

Under **Remember the PIN** you have three choices:

- **No, ask me each time** (the default),
- **Until I close KassenSturz**,
- **Save it in my keychain**: in your computer's own password store, the one your desktop or Windows provides.

In **Settings › Password storage** you set where remembered PINs are kept; the default (**System keychain**) is right for almost everyone. KassenSturz writes a PIN **nowhere else**, not into the book and not into any file. If your bank says the PIN is wrong, KassenSturz **forgets** it at once and asks once more; after a second wrong PIN it stops, so your login isn't locked. With **Forget the PIN** and **Remove bank login and saved PIN…** you tidy up later. Your accounts and transactions are kept.

## Getting new transactions

1. Choose **Get new transactions** on the accounts page or in an account's list.
2. Each account shows its bank account and the day it was last fetched, or **Link to a bank account…**.
3. Tick the accounts and choose **Get new transactions** again. KassenSturz asks for what is new, one account after another; for the PIN only if it doesn't know it.
4. **Nothing is added yet.** You see the same preview as for a file (page 10) and decide. Only **Import** adds the transactions, and **Undo** takes them back together with the day they were fetched.

A fetch starts five days before the last one, because banks sometimes book a day late; KassenSturz recognises what is there twice.

## The reminder

If a linked account hasn't been fetched for a while, the overview warns you with the account's name, the day and the number of days. Banks only show their most recent transactions, so a long wait can leave gaps. In **Settings › Reminder to get new transactions** you choose 30, 60 (the default), 90 or 180 days, or **Never remind me**. KassenSturz never contacts your bank by itself.

## What is saved

The book only remembers which bank account fills which account (by IBAN) and the day it was last fetched. **No login name and no PIN** is in the book, in backups or in a saved copy.

**If something looks different:** if a line under the account says what went wrong, for example that the bank couldn't be reached or the PIN was wrong, follow the hint there. Never ask anyone for your PIN, not us either.
