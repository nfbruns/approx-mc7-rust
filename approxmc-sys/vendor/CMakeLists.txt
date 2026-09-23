cmake_minimum_required(VERSION 3.18 FATAL_ERROR)
project(approxmc_vendor NONE)

# Fetches and builds ApproxMC (and, transitively, CryptoMiniSat5/Arjun/SBVA/
# cadical/cadiback via ApproxMC's own FetchContent) directly from GitHub at
# `cargo build` time -- no manual clone step. Everything is built as static
# (.a) libraries with -fPIC so the final Rust binary needs no rpath: the
# archives are embedded directly into it instead of loaded from a shared
# object at runtime.
#
# CACHE ... FORCE is required (not plain `set`) because ApproxMC's own
# CMakeLists.txt declares these same names via `option()`; a normal variable
# set here would otherwise be shadowed once FetchContent adds that
# subdirectory. Cache entries win regardless of directory scope.
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(STATIC_BINARY OFF CACHE BOOL "" FORCE)
set(BUILD_PYTHON_EXTENSION OFF CACHE BOOL "" FORCE)
set(ENABLE_TESTING OFF CACHE BOOL "" FORCE)
set(CMAKE_POSITION_INDEPENDENT_CODE ON CACHE BOOL "" FORCE)

set(APPROXMC_REPO "https://github.com/meelgroup/approxmc.git" CACHE STRING
    "Git URL to fetch ApproxMC from")
set(APPROXMC_TAG "master" CACHE STRING
    "Git branch/tag/commit of ApproxMC to build")

include(FetchContent)
FetchContent_Declare(approxmc
    GIT_REPOSITORY "${APPROXMC_REPO}"
    GIT_TAG        "${APPROXMC_TAG}"
    GIT_SHALLOW    TRUE
)
FetchContent_MakeAvailable(approxmc)
