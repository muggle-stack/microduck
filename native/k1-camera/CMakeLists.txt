cmake_minimum_required(VERSION 3.16)
project(microduck_k1_camera LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
find_package(OpenCV 4.14 REQUIRED COMPONENTS core imgproc)
find_package(Threads REQUIRED)
if(NOT MPP_SOURCE_DIR OR NOT MPP_LIBRARY_DIR OR NOT CAMERA_OUTPUT_DIR)
    message(FATAL_ERROR "Use scripts/build-k1-camera.sh for an isolated, pinned MPP build")
endif()
add_library(microduck_k1_camera SHARED camera_bridge.cpp)
target_include_directories(microduck_k1_camera PRIVATE
    "${MPP_SOURCE_DIR}/include" "${MPP_SOURCE_DIR}/include/sys"
    "${MPP_SOURCE_DIR}/include/uvc" "${MPP_SOURCE_DIR}/include/vdec"
    "${MPP_SOURCE_DIR}/include/v2d" ${OpenCV_INCLUDE_DIRS})
target_compile_options(microduck_k1_camera PRIVATE -Wall -Wextra -Werror -O3)
target_link_libraries(microduck_k1_camera PRIVATE
    "${MPP_LIBRARY_DIR}/libmpp.so" ${OpenCV_LIBS} Threads::Threads)
set_target_properties(microduck_k1_camera PROPERTIES
    LIBRARY_OUTPUT_DIRECTORY "${CAMERA_OUTPUT_DIR}"
    BUILD_RPATH "\$ORIGIN/lib;/opt/opencv-spacemit/lib"
    BUILD_WITH_INSTALL_RPATH TRUE INSTALL_RPATH "\$ORIGIN/lib;/opt/opencv-spacemit/lib")
add_executable(camera-native-check native_check.cpp)
target_compile_options(camera-native-check PRIVATE -Wall -Wextra -Werror)
target_link_libraries(camera-native-check PRIVATE microduck_k1_camera)
set_target_properties(camera-native-check PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CAMERA_OUTPUT_DIR}"
    BUILD_WITH_INSTALL_RPATH TRUE INSTALL_RPATH "\$ORIGIN")
add_executable(camera-image-test camera_bridge.cpp)
target_compile_definitions(camera-image-test PRIVATE MICRODUCK_IMAGE_TEST=1)
target_include_directories(camera-image-test PRIVATE
    $<TARGET_PROPERTY:microduck_k1_camera,INCLUDE_DIRECTORIES>)
target_compile_options(camera-image-test PRIVATE -Wall -Wextra -Werror -O3)
target_link_libraries(camera-image-test PRIVATE
    "${MPP_LIBRARY_DIR}/libmpp.so" ${OpenCV_LIBS} Threads::Threads)
set_target_properties(camera-image-test PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CAMERA_OUTPUT_DIR}"
    BUILD_WITH_INSTALL_RPATH TRUE INSTALL_RPATH "\$ORIGIN/lib;/opt/opencv-spacemit/lib")
