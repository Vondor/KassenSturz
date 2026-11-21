# 23. Plugins

*A plugin is a small file that adds something to KassenSturz. This page says what plugins are, how to add one safely and which ones the project points to. You never need a plugin: KassenSturz is complete without any.*

> **Not everything here is settled yet.** How plugins are chosen, listed and passed on is still being decided, so this page may change.

## What a plugin is

A plugin is one file, ending in `.wasm`, that someone wrote to give KassenSturz one more ability. A plugin can:

- read another kind of bank file,
- save your data in another kind of file,
- add a report,
- react to new transactions, for example after an import, and suggest changes,
- connect another AI service (only while the AI features are on),
- change the colors and fonts, or how much the screens explain.

A plugin runs closed off from the rest of your computer. It does only what its kind allows and what you saw when you added it. It never changes your book by itself: a change it suggests is shown to you first, and only your confirmation makes it.

## Which plugins there are

There is no shop and no list inside the app. This page names only plugins the project has chosen; an entry can be a link to another website where its author keeps it.

**No plugin has been chosen yet.** When the first ones are, they are listed here with what they do, who makes them and where to get them.

A plugin that proves itself may, if wanted, become part of KassenSturz itself. Then you no longer need to add it.

## Adding a plugin

1. Download the plugin file to your computer, for example into your **Downloads** folder. Take plugins only from people you trust.
2. In KassenSturz, open **Settings › Plugins**.
3. Choose **Add a plugin file…** and pick the file.
4. KassenSturz shows **Add this plugin?** with what the plugin adds and what it asks to be allowed, for example to read your book. Read it. Nothing has been added yet.
5. **Add it** adds the plugin. **Don't add it** changes nothing.

The file you downloaded is not changed, and you can delete it afterwards. If you add a plugin you already have, KassenSturz says so and replaces the old one.

## Removing a plugin

Under **Settings › Plugins › Your plugins**, choose **Remove this plugin…**. KassenSturz deletes its copy of the plugin. Your book isn't changed. To use the plugin again, you add its file again.

## Writing a plugin

That is for people who program. The guide is [Writing plugins](../../WRITING_PLUGINS.md), and the reference is [Plugins](../../PLUGINS.md).

**If something looks different:** if KassenSturz says a plugin can't be used any more, it was made for another version of KassenSturz. Remove it and look for a newer version of it.
