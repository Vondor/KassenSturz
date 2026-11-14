#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Warns when a translation looks like it was not taken seriously (D-778).

    python3 tools/check-translation-sense.py                 # every po file
    python3 tools/check-translation-sense.py --base <commit> # only entries changed since <commit>
    python3 tools/check-translation-sense.py --self-test     # checks this script on made-up files

tools/check-translations.sh decides whether a translation is complete and
valid, and CI fails without it. This script never fails: it only points a
reviewer at entries that look like a joke, vandalism, filler or a text that was
never really translated, so that a person looks at them before merging. It has
no way to understand a language, so every hit is a hint, not a verdict, and a
real translation may get one now and then.

What it looks for in each msgstr:

- key words (Cancel, Save, Delete, ...) whose translation doesn't contain any
  of the usual words for them in that language (wrong meaning);
- text left in English or copied from the English;
- letters of another writing system than the language uses (e.g. Latin letters
  in a Russian file);
- one translation used for many different English texts (filler);
- joke words, keyboard mashing, emoji and rude words;
- a translation far shorter or longer than the English;
- links or e-mail addresses the English text doesn't have;
- markup or line breaks that the English text has and the translation lost.

Inside GitHub Actions it writes warning annotations (GitHub shows at most ten
per step, so the most serious come first) and the full list to the job summary.
The exit code is always 0.
"""

import argparse
import os
import re
import subprocess
import sys
import tempfile
import unicodedata
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Key words and the word stems a real translation of them contains, by
# language. A stem matches anywhere in the lower-cased translation, so
# "speicher" covers "Speichern" and "speichert". A language not listed here
# skips this check. Several stems per word, because translators choose
# different good words; a missing good one only costs a warning.
ANCHORS = {
    "Cancel": {
        "de": ["abbrech", "abbruch"], "fr": ["annul"], "es": ["cancel"], "pt": ["cancel"],
        "it": ["annull"], "nl": ["annuleer"], "pl": ["anuluj"], "ru": ["отмен"],
        "uk": ["скасув", "відмін"], "id": ["batal"], "zh": ["取消"], "ja": ["キャンセル", "取り消", "中止"],
        "ko": ["취소"], "ar": ["إلغاء", "الغاء", "ألغ"], "hi": ["रद्द"], "bn": ["বাতিল"],
        "ur": ["منسوخ", "کینسل"],
    },
    "Save": {
        "de": ["speicher", "sichern"], "fr": ["enregistr", "sauvegard"], "es": ["guardar"],
        "pt": ["salvar", "guardar", "gravar"], "it": ["salva"], "nl": ["opslaan", "bewaar"],
        "pl": ["zapisz"], "ru": ["сохран"], "uk": ["збер"], "id": ["simpan"],
        "zh": ["保存", "存储"], "ja": ["保存"], "ko": ["저장"], "ar": ["حفظ", "احفظ"],
        "hi": ["सहेज", "सेव"], "bn": ["সংরক্ষণ", "সেভ"], "ur": ["محفوظ", "سیو"],
    },
    "Delete": {
        "de": ["lösch", "entfern"], "fr": ["supprim", "effac"], "es": ["elimin", "borrar", "suprim"],
        "pt": ["exclu", "apagar", "elimin", "remov"], "it": ["elimin", "cancella"],
        "nl": ["verwijder", "wis"], "pl": ["usuń", "usun", "usuw", "skasuj"], "ru": ["удал"],
        "uk": ["видал", "вилуч"], "id": ["hapus"], "zh": ["删除"], "ja": ["削除"], "ko": ["삭제"],
        "ar": ["حذف", "احذف", "إزالة", "ازالة"], "hi": ["हटा", "मिटा"], "bn": ["মুছ", "মোছ"],
        "ur": ["حذف", "مٹا", "ڈیلیٹ"],
    },
    "Undo": {
        "de": ["rückgängig"], "fr": ["annul", "défaire"], "es": ["deshacer"], "pt": ["desfazer", "anular"],
        "it": ["annulla"], "nl": ["ongedaan"], "pl": ["cofnij"], "ru": ["отмен", "вернуть"],
        "uk": ["скасув", "відмін", "повернути"], "id": ["urung", "batalkan"],
        "zh": ["撤销", "撤消", "还原"], "ja": ["元に戻", "取り消"], "ko": ["실행 취소", "되돌", "취소"],
        "ar": ["تراجع"], "hi": ["पूर्ववत", "वापस"], "bn": ["পূর্বাবস্থা", "আনডু", "ফিরিয়ে"],
        "ur": ["واپس", "کالعدم", "انڈو"],
    },
    "Settings": {
        "de": ["einstellung"], "fr": ["paramètre", "réglage", "préférence", "option"],
        "es": ["ajuste", "configuraci", "preferencia"], "pt": ["configura", "definiç", "preferência", "ajuste"],
        "it": ["impostazion"], "nl": ["instelling"], "pl": ["ustawieni"], "ru": ["настройк", "параметр"],
        "uk": ["налаштув", "параметр"], "id": ["pengaturan", "setelan"], "zh": ["设置"], "ja": ["設定"],
        "ko": ["설정"], "ar": ["إعدادات", "اعدادات"], "hi": ["सेटिंग"], "bn": ["সেটিং"],
        "ur": ["ترتیبات", "سیٹنگ"],
    },
    "Date": {
        "de": ["datum"], "fr": ["date"], "es": ["fecha"], "pt": ["data"], "it": ["data"], "nl": ["datum"],
        "pl": ["data"], "ru": ["дата"], "uk": ["дата"], "id": ["tanggal"], "zh": ["日期"],
        "ja": ["日付"], "ko": ["날짜", "일자"], "ar": ["تاريخ"], "hi": ["तारीख", "तिथि", "दिनांक"],
        "bn": ["তারিখ"], "ur": ["تاریخ"],
    },
    "Amount": {
        "de": ["betrag"], "fr": ["montant", "somme"], "es": ["importe", "monto", "cantidad"],
        "pt": ["valor", "montante", "quantia"], "it": ["importo", "somma"], "nl": ["bedrag"],
        "pl": ["kwot"], "ru": ["сумм"], "uk": ["сум"], "id": ["jumlah", "nominal"],
        "zh": ["金额", "数额"], "ja": ["金額"], "ko": ["금액"], "ar": ["مبلغ"],
        "hi": ["राशि", "रकम"], "bn": ["পরিমাণ", "টাকা"], "ur": ["رقم"],
    },
}

# The writing system each language uses. Languages not listed are written in
# Latin letters (this includes Klingon and Sindarin in their usual Latin
# transcription).
SCRIPTS = {
    "ru": {"CYRILLIC"}, "uk": {"CYRILLIC"}, "be": {"CYRILLIC"}, "bg": {"CYRILLIC"},
    "sr": {"CYRILLIC", "LATIN"}, "mk": {"CYRILLIC"}, "kk": {"CYRILLIC"},
    "zh": {"CJK"}, "ja": {"CJK", "HIRAGANA", "KATAKANA"}, "ko": {"HANGUL", "CJK"},
    "ar": {"ARABIC"}, "ur": {"ARABIC"}, "fa": {"ARABIC"}, "he": {"HEBREW"},
    "hi": {"DEVANAGARI"}, "mr": {"DEVANAGARI"}, "ne": {"DEVANAGARI"}, "bn": {"BENGALI"},
    "el": {"GREEK"}, "th": {"THAI"}, "ka": {"GEORGIAN"}, "hy": {"ARMENIAN"},
}
# Languages whose texts are much shorter in characters than the English.
DENSE = {"zh", "ja", "ko"}

# Words that only turn up in a translation as a joke or as vandalism. Whole
# words, lower case. "*" applies to every language, a language code only to
# that language; EXCEPT lists ordinary words of a language that look like one.
RUDE = {
    "*": """fuck fucking fucked fucker motherfucker shit shitty bullshit asshole arsehole
        bitch cunt dick dickhead cock wanker twat piss pissed crap slut whore bollocks
        nigger nigga faggot retarded porn porno penis hitler nazi
        scheiße scheisse arschloch fotze wichser hurensohn
        merde putain mierda joder cazzo vaffanculo kurwa caralho porra
        хуй пизда блядь бля сука""".split(),
    "de": "arsch fick ficken gefickt kacke pisse schlampe nutte schwuchtel spast spasti missgeburt vollidiot penner".split(),
    "fr": "connard connasse salope enculé encule pute bite couilles nique foutre chier".split(),
    "es": "puta puto coño gilipollas cabrón cabron pendejo verga culo polla maricón".split(),
    "it": "stronzo puttana minchia coglione figa".split(),
    "pt": "foda foder buceta viado".split(),
    "nl": "kut klootzak lul tering godverdomme hoer neuken".split(),
    "pl": "chuj jebać jebac pizda skurwysyn spierdalaj pierdolę pierdole".split(),
    "ru": "ебать ебан мудак говно пидор хер".split(),
    "uk": "ебать мудак гівно лайно".split(),
    "tlh": "petaq taHqeq yIntagh".split(),
}
EXCEPT = {
    "de": {"dick"}, "nl": {"dick"}, "sv": {"slut", "fart", "kuk"}, "da": {"slut", "fart"},
    "nb": {"slut", "fart"}, "nn": {"slut", "fart"}, "no": {"slut", "fart"},
    "es": {"porra"}, "pt": {"porra"},
}

JOKES = [
    (r"\b(lol|lmao|rofl|roflmao|xd+|omg|wtf|kek|lulz|yolo|bruh)\b", "a joke or chat word"),
    (r"\b(ha){3,}\b|\b(he){3,}\b|\b(hi){3,}\b", "laughter"),
    (r"\b(asdf\w*|qwert\w*|jkl\w*|fdsa|lorem ipsum|blah|bla bla|foo|bar baz)\b", "filler or keyboard mashing"),
    (r"\b(todo|fixme|xxx|tbd|translate me|dunno|no idea)\b", "a note instead of a translation"),
    (r"(\w)\1{4,}", "one letter repeated many times"),
    (r"[!?]{3,}", "a row of ! or ?"),
]
LINK = re.compile(r"(?:https?://|www\.)[\w-]+\.[\w./?=&%-]+|\b[\w.+-]+@[\w-]+\.[\w.]+\b", re.I)
PLACEHOLDER = re.compile(r"%\d+|%[sdi]|\\n|<[^>]+>|&\w+;|\*\.\w+")
TAG = re.compile(r"</?\w+[^>]*>")
WORD = re.compile(r"[^\W\d_]+(?:['’][^\W\d_]+)?")

SEVERITY = {"rude": 0, "meaning": 1, "joke": 2, "link": 2, "script": 3, "english": 4,
            "filler": 5, "markup": 6, "length": 7}


class Entry:
    def __init__(self):
        self.ctxt = None
        self.msgid = None
        self.plural = None
        self.msgstr = {}
        self.line = 0          # line of the first msgstr
        self.lines = (0, 0)    # first and last line of the entry
        self.obsolete = False
        self.fuzzy = False


def unquote(s):
    s = s.strip()
    if not (s.startswith('"') and s.endswith('"')):
        return ""
    s = s[1:-1]
    return re.sub(r'\\(.)', lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), s)


def parse_po(text):
    entries, cur, field, start = [], None, None, 0
    lines = text.split("\n")

    def finish(end):
        nonlocal cur
        if cur is not None and cur.msgid is not None:
            cur.lines = (start, end)
            entries.append(cur)
        cur = None

    for n, raw in enumerate(lines, 1):
        line = raw.strip()
        if not line:
            finish(n - 1)
            field = None
            continue
        if cur is not None and cur.msgstr and not line.startswith('"') and not line.startswith("msgstr"):
            # The next entry starts without a blank line in between, which
            # msgfmt accepts: end this one here so neither is lost.
            finish(n - 1)
            field = None
        if cur is None:
            cur, start = Entry(), n
        if line.startswith("#~"):
            cur.obsolete = True
            continue
        if line.startswith("#,") and "fuzzy" in line:
            cur.fuzzy = True
        if line.startswith("#"):
            continue
        m = re.match(r'(msgctxt|msgid_plural|msgid|msgstr(?:\[(\d+)\])?)\s+(".*")$', line)
        if m:
            key, idx, val = m.group(1), m.group(2), unquote(m.group(3))
            if key == "msgctxt":
                cur.ctxt, field = val, ("ctxt", None)
            elif key == "msgid":
                cur.msgid, field = val, ("msgid", None)
            elif key == "msgid_plural":
                cur.plural, field = val, ("plural", None)
            else:
                i = int(idx) if idx is not None else 0
                if not cur.msgstr:
                    cur.line = n
                cur.msgstr[i], field = val, ("msgstr", i)
        elif line.startswith('"') and field:
            val = unquote(line)
            kind, i = field
            if kind == "ctxt":
                cur.ctxt += val
            elif kind == "msgid":
                cur.msgid += val
            elif kind == "plural":
                cur.plural += val
            else:
                cur.msgstr[i] += val
    finish(len(lines))
    return [e for e in entries if e.msgid and not e.obsolete]


def script_of(ch):
    try:
        name = unicodedata.name(ch)
    except ValueError:
        return None
    for s in ("LATIN", "CYRILLIC", "CJK", "HIRAGANA", "KATAKANA", "HANGUL", "ARABIC", "HEBREW",
              "DEVANAGARI", "BENGALI", "GREEK", "THAI", "GEORGIAN", "ARMENIAN"):
        if name.startswith(s) or (s == "CJK" and "IDEOGRAPH" in name):
            return s
    return "OTHER"


def is_emoji(ch):
    o = ord(ch)
    return 0x1F300 <= o <= 0x1FAFF or 0x2600 <= o <= 0x27BF


def words(s):
    return [w.lower() for w in WORD.findall(s)]


def visible_len(s, lang):
    s = PLACEHOLDER.sub("", s)
    n = 0
    for ch in s:
        if ch.isspace():
            continue
        sc = script_of(ch) if ord(ch) > 0x2E7F else None
        n += 2.5 if (lang in DENSE and sc in ("CJK", "HIRAGANA", "KATAKANA", "HANGUL")) else 1
    return n


def check_entry(e, lang, text):
    """Findings for one translated text: list of (kind, message)."""
    out = []
    src = e.msgid if text is e.msgstr.get(0) or e.plural is None else e.plural
    low = text.lower()
    src_words = set(words(e.msgid) + words(e.plural or ""))

    bad = set(RUDE["*"]) | set(RUDE.get(lang, []))
    bad -= EXCEPT.get(lang, set())
    hits = sorted({w for w in words(text) if w in bad and w not in src_words})
    if hits:
        out.append(("rude", "rude or offensive word: " + ", ".join(hits)))

    for pattern, what in JOKES:
        m = re.search(pattern, low)
        if m and not re.search(pattern, src.lower()):
            out.append(("joke", f"{what}: \u201c{m.group(0)}\u201d"))
            break
    emoji = [ch for ch in text if is_emoji(ch) and ch not in src]
    if emoji:
        out.append(("joke", "emoji the English text doesn't have: " + "".join(emoji[:5])))

    links = [m.group(0) for m in LINK.finditer(text) if m.group(0).lower() not in src.lower()]
    if links:
        out.append(("link", "link or e-mail address the English text doesn't have: " + links[0]))

    anchor = ANCHORS.get(e.msgid, {}).get(lang)
    if anchor and not any(stem in low for stem in anchor):
        out.append(("meaning", f"\u201c{e.msgid}\u201d is usually translated with a word like "
                    + " / ".join(anchor) + "; please check the meaning"))

    if lang != "en":
        tw = [w for w in words(PLACEHOLDER.sub(" ", text)) if len(w) > 2]
        if text.strip() == src.strip() and len(tw) >= 3:
            out.append(("english", "left in English (same as the English text)"))
        elif len(tw) >= 4:
            same = sum(1 for w in tw if w in src_words)
            if same / len(tw) >= 0.75:
                out.append(("english", "mostly English words copied from the English text"))

    # Writing system: count letters that aren't simply copied from the English
    # (names such as KassenSturz, CSV or IBAN stay as they are).
    expected = SCRIPTS.get(lang, {"LATIN"})
    counts = defaultdict(int)
    for w in WORD.findall(PLACEHOLDER.sub(" ", text)):
        if w.lower() in src_words:
            continue
        for ch in w:
            sc = script_of(ch)
            if sc:
                counts[sc] += 1
    total = sum(counts.values())
    if total >= 6:
        foreign = sum(v for k, v in counts.items() if k not in expected)
        if foreign / total > 0.5:
            main = max((k for k in counts if k not in expected), key=counts.get)
            out.append(("script", f"mostly {main.lower()} letters, but this language is written in "
                        + " / ".join(sorted(s.lower() for s in expected))))

    if TAG.findall(src) and len(TAG.findall(text)) < len(TAG.findall(src)):
        out.append(("markup", "the English text has markup (<b>...) that the translation lost"))
    if src.count("\n") and text.count("\n") < src.count("\n"):
        out.append(("markup", "the English text has line breaks that the translation lost"))

    a, b = visible_len(src, "en"), visible_len(text, lang)
    if a >= 15 and b and (b / a < 0.25 or b / a > 4) and abs(b - a) > 12:
        out.append(("length", f"{int(b)} characters for an English text of {int(a)}; please check it says the same"))
    return out


def check_file(path, rel, changed):
    """All findings in one po file, as (line, kind, message)."""
    lang_dir = Path(rel).parent.name
    lang = re.split(r"[_@.]", lang_dir)[0].lower()
    entries = parse_po(Path(path).read_text(encoding="utf-8", errors="replace"))
    findings = []

    def wanted(e):
        return changed is None or any(e.lines[0] <= n <= e.lines[1] for n in changed)

    # One translation for many different English texts.
    users = defaultdict(set)
    for e in entries:
        for s in e.msgstr.values():
            norm = " ".join(words(s))
            if norm:
                users[norm].add(e.msgid.lower())

    for e in entries:
        if not wanted(e):
            continue
        for i, s in sorted(e.msgstr.items()):
            if not s.strip():
                continue
            for kind, msg in check_entry(e, lang, s):
                findings.append((e.line, kind, msg, e.msgid))
            norm = " ".join(words(s))
            if norm and len(users[norm]) >= 6:
                findings.append((e.line, "filler", f"the same translation \u201c{s[:40]}\u201d is used for "
                                 f"{len(users[norm])} different English texts", e.msgid))
                users[norm] = set()  # report each filler text once
    return findings


def changed_lines(base, rel):
    try:
        diff = subprocess.run(["git", "diff", "-U0", f"{base}...HEAD", "--", rel], cwd=ROOT,
                              capture_output=True, text=True, check=True).stdout
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None  # can't tell: check the whole file
    lines = set()
    for m in re.finditer(r"^@@ [^+]*\+(\d+)(?:,(\d+))? @@", diff, re.M):
        start, count = int(m.group(1)), int(m.group(2) or 1)
        lines.update(range(start, start + count))
    return lines


def escape(s):
    return s.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def report(results):
    gh = os.environ.get("GITHUB_ACTIONS") == "true"
    allf = [(rel, *f) for rel, fs in results for f in fs]
    allf.sort(key=lambda f: (SEVERITY[f[2]], f[0], f[1]))
    if not allf:
        print("check-translation-sense: nothing looks odd.")
    for n, (rel, line, kind, msg, msgid) in enumerate(allf):
        short = msgid if len(msgid) <= 60 else msgid[:57] + "..."
        text = f"{msg} (English: \u201c{short}\u201d)"
        if gh and n < 10:
            print(f"::warning file={rel},line={line},title=Translation looks odd ({kind})::{escape(text)}")
        else:
            print(f"{rel}:{line}: [{kind}] {text}")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if gh and summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write("## Translation hints (never blocking, D-778)\n\n")
            if not allf:
                f.write("Nothing looks odd in the changed translations.\n")
            else:
                f.write(f"{len(allf)} entries look odd. These are hints for a reviewer, not errors: "
                        "please read them before merging.\n\n| File | Line | Kind | Hint |\n|---|---|---|---|\n")
                for rel, line, kind, msg, msgid in allf[:300]:
                    cell = f"{msg} (English: {msgid[:60]})".replace("|", "\\|").replace("\n", " ")
                    f.write(f"| {rel} | {line} | {kind} | {cell} |\n")
                if len(allf) > 300:
                    f.write(f"\n... and {len(allf) - 300} more; run the script locally for all.\n")
    return allf


def run(base, files=None):
    rels = files or sorted(str(p.relative_to(ROOT)) for p in (ROOT / "po").glob("*/kassensturz.po"))
    results = []
    for rel in rels:
        path = ROOT / rel
        if not path.exists():
            continue
        changed = changed_lines(base, rel) if base else None
        if changed is not None and not changed:
            continue
        try:
            results.append((rel, check_file(path, rel, changed)))
        except Exception as err:  # a hint tool must never break the build
            print(f"check-translation-sense: could not read {rel}: {err}")
    return report(results)


SAMPLE = '''msgid ""
msgstr ""
"Language: {lang}\\n"

msgctxt "@action:button"
msgid "Cancel"
msgstr "{cancel}"

msgid "Save"
msgstr "{save}"

msgid "Your money stays on this computer and nobody else sees it."
msgstr "{long}"

msgid "Delete"
msgstr "{delete}"
'''


def self_test():
    """Made-up files: an honest one gets no hint, each vandalised one its own."""
    cases = [
        ("de", dict(cancel="Abbrechen", save="Speichern", delete="Löschen",
                    long="Ihr Geld bleibt auf diesem Computer, und niemand sonst sieht es."), set()),
        ("ru", dict(cancel="Отмена", save="Сохранить", delete="Удалить",
                    long="Ваши деньги остаются на этом компьютере, и никто их не видит."), set()),
        ("ja", dict(cancel="キャンセル", save="保存", delete="削除",
                    long="お金のデータはこのパソコンに残り、他の人には見えません。"), set()),
        ("de", dict(cancel="fuck", save="Speichern", delete="Löschen",
                    long="Ihr Geld bleibt auf diesem Computer, und niemand sonst sieht es."), {"rude", "meaning"}),
        ("fr", dict(cancel="Annuler", save="Enregistrer", delete="Supprimer",
                    long="Your money stays on this computer and nobody else sees it."), {"english"}),
        ("ru", dict(cancel="Отмена", save="Сохранить", delete="Удалить",
                    long="Vashi dengi ostayutsya na etom kompyutere i nikto ikh ne vidit."), {"script"}),
        ("es", dict(cancel="Cancelar", save="Guardar", delete="Eliminar", long="jajaja lol"),
         {"joke", "length"}),
        ("pl", dict(cancel="Anuluj", save="Zapisz", delete="Usuń",
                    long="Kup tanio na www.example.invalid teraz"), {"link"}),
        ("it", dict(cancel="Annulla", save="Pizza", delete="Elimina",
                    long="I tuoi soldi restano su questo computer e nessun altro li vede."), {"meaning"}),
    ]
    ok = True
    with tempfile.TemporaryDirectory() as tmp:
        for n, (lang, values, want) in enumerate(cases):
            rel = f"po/{lang}/kassensturz.po"
            path = Path(tmp) / f"{n}.po"
            path.write_text(SAMPLE.format(lang=lang, **values), encoding="utf-8")
            got = {f[1] for f in check_file(path, rel, None)}
            if got != want:
                print(f"self-test case {n} ({lang}): expected {sorted(want)}, got {sorted(got)}")
                ok = False
        # Filler: one word for many different texts.
        body = '\n'.join(f'msgid "Text number {i} is different"\nmsgstr "Blabla"\n' for i in range(8))
        path = Path(tmp) / "filler.po"
        path.write_text(body, encoding="utf-8")
        got = [f[1] for f in check_file(path, "po/de/kassensturz.po", None)]
        if got.count("filler") != 1:
            print(f"self-test filler: expected one filler hint, got {got}")
            ok = False
        # Two entries without a blank line between them are both read.
        joined = parse_po('msgid "One"\nmsgstr "Eins"\n#, kde-format\nmsgid "Two"\nmsgstr "Zwei"\n')
        if [e.msgid for e in joined] != ["One", "Two"]:
            print(f"self-test parse: expected One and Two, got {[e.msgid for e in joined]}")
            ok = False
    print("self-test passed" if ok else "self-test FAILED")
    return 0 if ok else 1


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--base", help="only check entries changed since this commit")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("files", nargs="*", help="po files, relative to the repository (default: all)")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    run(args.base, args.files)
    return 0


if __name__ == "__main__":
    sys.exit(main())
