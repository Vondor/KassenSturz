# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: BSD-2-Clause

# ctest for StampMetainfo.cmake (M6-B156): the newest <release> gets the
# build's number and date, nothing else changes, and an unknown number
# leaves the file as it is.

set(work "${CMAKE_CURRENT_LIST_DIR}/../build-stamp-test")
if(DEFINED WORK)
    set(work "${WORK}")
endif()
file(MAKE_DIRECTORY "${work}")
set(in "${work}/in.xml")
file(WRITE "${in}" "<component>\n  <releases>\n    <release version=\"0.261025.0\" date=\"2026-10-25\">\n      <description><p>x</p></description>\n    </release>\n  </releases>\n</component>\n")

execute_process(COMMAND "${CMAKE_COMMAND}" -DIN=${in} -DOUT=${work}/out.xml -DVERSION=0.261103.32
    -P "${CMAKE_CURRENT_LIST_DIR}/StampMetainfo.cmake" RESULT_VARIABLE result)
file(READ "${work}/out.xml" out)
set(expected "<component>\n  <releases>\n    <release version=\"0.261103.32\" date=\"2026-11-03\">\n      <description><p>x</p></description>\n    </release>\n  </releases>\n</component>\n")
if(NOT result EQUAL 0 OR NOT out STREQUAL expected)
    message(FATAL_ERROR "stamping 0.261103.32 gave:\n${out}")
endif()

execute_process(COMMAND "${CMAKE_COMMAND}" -DIN=${in} -DOUT=${work}/same.xml -DVERSION=
    -P "${CMAKE_CURRENT_LIST_DIR}/StampMetainfo.cmake" RESULT_VARIABLE result)
file(READ "${in}" original)
file(READ "${work}/same.xml" same)
if(NOT result EQUAL 0 OR NOT same STREQUAL original)
    message(FATAL_ERROR "an empty number changed the file:\n${same}")
endif()
