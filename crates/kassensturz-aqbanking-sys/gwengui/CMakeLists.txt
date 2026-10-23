# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: BSD-2-Clause

# kassensturz-gwengui: KassenSturz's small C++ library around Gwenhywfar's
# Qt 6 dialogs (Q-035 option A, D-140), loaded at runtime like AqBanking
# itself. Optional: without the gwengui-qt6 development package the app
# builds and runs without bank dialogs (D-054); M4.8's Flatpak bundles it.

find_package(PkgConfig REQUIRED)
pkg_check_modules(GWENGUI_QT6 IMPORTED_TARGET gwengui-qt6>=5.14.1 gwenhywfar>=5.14.1)
add_feature_info("Bank dialogs" GWENGUI_QT6_FOUND
    "Gwenhywfar's Qt 6 dialogs for bank setup, PIN and TAN (gwengui-qt6 5.14.1 or newer)")
if(NOT GWENGUI_QT6_FOUND)
    return()
endif()

add_library(kassensturz-gwengui MODULE kassensturz_gwengui.cpp kassensturz_gwengui.h)
target_link_libraries(kassensturz-gwengui PRIVATE PkgConfig::GWENGUI_QT6 Qt6::Widgets)
target_compile_definitions(kassensturz-gwengui PRIVATE KS_GWENGUI_BUILDING)
# Only the four C functions of kassensturz_gwengui.h are visible.
set_target_properties(kassensturz-gwengui PROPERTIES
    CXX_VISIBILITY_PRESET hidden
    VISIBILITY_INLINES_HIDDEN ON
)
install(TARGETS kassensturz-gwengui LIBRARY DESTINATION ${KDE_INSTALL_LIBDIR}/kassensturz)

# The test loads the built library the way KassenSturz does and drives
# Gwenhywfar's GUI entry points through it, on Qt's offscreen platform.
add_executable(kassensturz-gwengui-test kassensturz_gwengui_test.cpp)
target_link_libraries(kassensturz-gwengui-test PRIVATE PkgConfig::GWENGUI_QT6 Qt6::Widgets ${CMAKE_DL_LIBS})
add_test(NAME kassensturz-gwengui COMMAND kassensturz-gwengui-test $<TARGET_FILE:kassensturz-gwengui>)
set_tests_properties(kassensturz-gwengui PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
