# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: BSD-2-Clause

# Writes the AppStream file with the build's own number (M6-B156, D-226).
#
# The number of a build comes from git (tools/build-version.sh), not from a
# file, so the <release> entry in data/*.metainfo.xml keeps the number of the
# last build prepared by hand. Flatpak and software centres read the version
# from the installed AppStream file, so it showed that old number. This
# writes a copy whose newest <release> carries the build's number and date;
# the file in the repository stays as it is.
#
#   cmake -DIN=<metainfo.xml> -DOUT=<copy> -DVERSION=<0.YYMMDD.N> -P StampMetainfo.cmake
#
# A VERSION that isn't of the form 0.YYMMDD.N (empty when git can't tell)
# leaves the copy as the original.

file(READ "${IN}" metainfo)
if(VERSION MATCHES "^([0-9]+)\\.([0-9][0-9])([0-9][0-9])([0-9][0-9])\\.([0-9]+)$")
    set(date "20${CMAKE_MATCH_2}-${CMAKE_MATCH_3}-${CMAKE_MATCH_4}")
    string(FIND "${metainfo}" "<release version=\"" start)
    if(start EQUAL -1)
        message(FATAL_ERROR "${IN} has no <release version=...> entry to stamp")
    endif()
    string(SUBSTRING "${metainfo}" 0 ${start} before)
    string(SUBSTRING "${metainfo}" ${start} -1 rest)
    string(FIND "${rest}" ">" end)
    math(EXPR after_start "${end} + 1")
    string(SUBSTRING "${rest}" ${after_start} -1 after)
    set(metainfo "${before}<release version=\"${VERSION}\" date=\"${date}\">${after}")
endif()
file(WRITE "${OUT}" "${metainfo}")
