#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Makes a data-only plugin file (a theme, D-925, or a UI density plugin, D-927) from its JSON files.

A data-only plugin is a WebAssembly module with no code: only the
WebAssembly header and custom sections (D-849). This writes the manifest
as the section `kassensturz-manifest` and each further file as the section
named after it, exactly as the files are, so the plugin holds the same
bytes as the files:

    tools/make-data-plugin.py manifest.json kassensturz-theme=theme.json out.wasm

KassenSturz checks everything when the plugin is added; this only packs.
"""

import sys

HEADER = bytes([0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00])


def leb(value):
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value == 0:
            out.append(byte)
            return bytes(out)
        out.append(byte | 0x80)


def custom(name, data):
    name = name.encode("utf-8")
    content = leb(len(name)) + name + data
    return b"\x00" + leb(len(content)) + content


def main(args):
    if len(args) < 2:
        sys.exit(__doc__)
    *inputs, output = args
    manifest, *sections = inputs
    with open(manifest, "rb") as file:
        module = HEADER + custom("kassensturz-manifest", file.read())
    for section in sections:
        name, _, path = section.partition("=")
        if not path:
            sys.exit(f"{section}: expected NAME=FILE")
        with open(path, "rb") as file:
            module += custom(name, file.read())
    with open(output, "wb") as file:
        file.write(module)
    print(f"wrote {output}: {len(module)} bytes")


if __name__ == "__main__":
    main(sys.argv[1:])
