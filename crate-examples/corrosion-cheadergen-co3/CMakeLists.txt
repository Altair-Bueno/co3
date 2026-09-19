cmake_minimum_required(VERSION 3.16)

project(corrosion-cheadergen-co3 LANGUAGES C)

include(FetchContent)
include(CTest)
FetchContent_Declare(
    Corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG v0.6.1
)
FetchContent_MakeAvailable(Corrosion)

corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
    CRATES corrosion-cheadergen-co3
)

find_program(CHEADERGEN cheadergen REQUIRED)
set(GENERATED_INCLUDE_DIR "${CMAKE_CURRENT_BINARY_DIR}/include")
set(GENERATED_HEADER "${GENERATED_INCLUDE_DIR}/corrosion_cheadergen_co3.h")
get_filename_component(CO3_WORKSPACE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)

file(GLOB_RECURSE CO3_RUST_SOURCES CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/src/*.rs")
add_custom_command(
    OUTPUT "${GENERATED_HEADER}"
    COMMAND "${CHEADERGEN}" generate
            --output-dir "${GENERATED_INCLUDE_DIR}"
            --package corrosion-cheadergen-co3
    WORKING_DIRECTORY "${CO3_WORKSPACE_ROOT}"
    DEPENDS ${CO3_RUST_SOURCES} "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
    COMMENT "Generating C header from ffi! declarations"
    VERBATIM
)

add_custom_target(generate-header DEPENDS "${GENERATED_HEADER}")

add_executable(demo main.c)
add_dependencies(demo generate-header)
target_include_directories(demo PRIVATE "${GENERATED_INCLUDE_DIR}")
target_link_libraries(demo PRIVATE corrosion_cheadergen_co3)

enable_testing()
add_test(NAME demo COMMAND demo)
