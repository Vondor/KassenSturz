# Dashboard file format

A dashboard file holds **one dashboard**, so it can be given to another book: the user's own or someone else's (D-441, D-432). It holds **no money**: no amounts, no transactions, no balances, no IBANs and no IDs of the book. It does hold the names of the accounts, categories and groups the dashboard uses, its titles and heading texts, and the text of its formulas.

**Status: version 1** (M6d.3). KassenSturz writes it with *Save to a file…* in the dashboard menu; *Add from a file…* reads it (M6d.4, D-442). In code, `write_dashboard_file` in `kassensturz-report` writes the text and `replace_file` in `kassensturz-export` puts it on disk. Like the complete export (`docs/EXPORT_FORMAT.md`), the format is meant to be read and written by other programs too; it changes only with a new `version`.

## An example

```json
{
  "format": "kassensturz-dashboard-file",
  "version": 1,
  "application": "KassenSturz 0.261101.8",
  "references": {
    "ref:1": {
      "kind": "account",
      "name": "Colony Checking"
    },
    "ref:2": {
      "kind": "category",
      "path": [
        "Everyday",
        "Pets"
      ]
    },
    "ref:3": {
      "kind": "group",
      "name": "Fixed costs"
    }
  },
  "dashboard": {
    "format": "kassensturz-dashboard",
    "version": 3,
    "kind": "dashboard",
    "name": "Where the money goes",
    "template": "spending",
    "filters": {
      "period": "last-12-months",
      "accounts": [
        "ref:1"
      ]
    },
    "components": [
      {
        "id": "c1",
        "type": "chart.stacked_bar",
        "dataset": "category_over_time",
        "params": {
          "series": [
            "ref:2",
            "ref:3"
          ]
        },
        "props": {
          "period": "page"
        },
        "frame": {
          "col": 0,
          "row": 0,
          "cols": 12,
          "rows": 6
        }
      }
    ]
  }
}
```

(KassenSturz writes every property of a component it knows; the example leaves some out to stay short.)

## The file

- One JSON object (RFC 8259), UTF-8 without a byte order mark. The suggested name is the dashboard's name with `.dashboard.json`, each character a file system refuses (`/ \ : * ? " < > |` and control characters) replaced by `_`; the file chooser filters `*.json`. A reader knows the file by `format`, never by its name.
- At most **1 MiB**.
- Nesting at most 10 levels deep for the whole file (the document's own limit of 8 inside `dashboard`).
- No key twice in one object, and no key the format doesn't name (strict, like the document).

## The header

| Key | What it holds |
|---|---|
| `format` | Exactly `kassensturz-dashboard-file`. |
| `version` | A whole number from 1. A file whose version is newer than the program knows is refused (`report.unsupported-dashboard-file-version`). |
| `application` | The program and build that wrote the file, plain text on one line, at most 100 characters. Information only: never used to decide anything. |
| `references` | What each reference key names (below). An empty object when the dashboard names no account, category or group. |
| `dashboard` | The dashboard document (below). |

## `references`

Each key is `ref:` followed by a number from 1, and maps to one of:

- `{"kind": "category", "path": [...]}`: a category, by its names from the top-level category down. A path is a list, so no separator inside a name can break it. At most 16 names.
- `{"kind": "group", "name": ...}`: one of My groups.
- `{"kind": "account", "name": ...}`: one of the user's own accounts (closed ones too).

Every name is 1 to 200 characters of plain text, without control characters and without spaces at either end. At most **500** references; every key listed is used at least once, and every key used is listed. The same thing named twice gets one key.

## `dashboard`

The dashboard document exactly as the book keeps it (`docs/REPORT_DESIGNER.md`, D-278, D-289, D-351; version 3 since M6d.8, D-452), with these differences:

- **Every place that holds an ID of the book holds a reference key instead.** These places are the schema's ID lists (`params.groups`, `params.categories`, `params.accounts`, `params.series`), the `members` of each column in `params.columns` (D-339), and `filters.accounts` and `filters.categories`. A UUID or any other text in these places is refused (`id-in-file`), so a file can never point into a book by ID, not even by chance.
- **The default mark (`"default": true`, D-448) is never written**, and a reader refuses a file that has it (`unknown-property`, path `dashboard.default`), so a file can't change which dashboard opens. A view's `origin` (D-450) is never written either.
- **Formulas** already name categories and groups by name (D-292), so they are written as they are stored (D-295) and need no references.
- The document's own limits apply: 100 components, 50 series and the rest of D-281.
- A component of a type the writing program doesn't know is kept as it is (D-278); its settings hold no IDs this program could know of.

## What is left out (D-458)

An ID in the dashboard that names nothing the book still has (a category deleted since), or whose name a reader would refuse (longer than 200 characters, with a control character or spaces at the ends, a path deeper than 16, more than 500 references), is **left out** of its list. The file is then always one that KassenSturz reads. *Save to a file* says how many were left out. A list of the filter bar that is empty after this is left out with it; a component's list stays, empty.

## How KassenSturz writes it

- Keys in the order this document shows them, then any other key alphabetically: the header as above; in `dashboard` `format`, `version`, `kind`, `name`, `template`, `filters`, `components`; in `filters` `period`, `accounts`, `categories`; in a component `id`, `type`, `dataset`, `params`, `props`, `frame`, with the keys inside `params` and `props` alphabetical; in a column `name`, `members`, `formula`; in a frame `col`, `row`, `cols`, `rows`; in a reference `kind`, then `path` or `name`; the references by their number.
- Two spaces per level, every non-empty object or list over several lines, `{}` and `[]` when empty, a line break at the end.
- References are numbered in the order they are first met in this order of keys.
- So the same dashboard in the same book gives the same file, byte for byte.
- On disk the text goes first into a new temporary file next to the chosen one (`.<name>.<n>.partial`), which is then renamed over it, so a failed write leaves the old file as it was. Where the folder doesn't allow that (the file chooser's portal gives the app only the chosen file, D-125), the file is written in place, and a failed write leaves it empty or removes it. Either way there is never a half-written file; the error is `export.io`. An existing file is replaced, because the file chooser asks first (D-443).

## Versions

| Version | Since | What changed |
|---|---|---|
| 1 | M6d.3 | First version. |
