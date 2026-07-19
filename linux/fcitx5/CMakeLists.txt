cmake_minimum_required(VERSION 3.16)
project(entropy-fcitx5 LANGUAGES CXX)

include(CTest)

find_package(Fcitx5Core REQUIRED)
find_package(Fcitx5Module REQUIRED)

add_fcitx5_addon(entropyuniversalsymbols src/entropyuniversalsymbols.cpp)
target_link_libraries(entropyuniversalsymbols Fcitx5::Core)

if(BUILD_TESTING)
    add_executable(entropy-fcitx5-host-text-test src/entropyuniversalsymbols.cpp)
    target_compile_definitions(entropy-fcitx5-host-text-test PRIVATE ENTROPY_HOST_TEXT_TEST)
    target_link_libraries(entropy-fcitx5-host-text-test Fcitx5::Core)
    add_test(NAME entropy-fcitx5-host-text COMMAND entropy-fcitx5-host-text-test)
endif()
install(TARGETS entropyuniversalsymbols DESTINATION "${FCITX_INSTALL_ADDONDIR}")
install(FILES entropyuniversalsymbols.conf DESTINATION "${FCITX_INSTALL_PKGDATADIR}/addon")
