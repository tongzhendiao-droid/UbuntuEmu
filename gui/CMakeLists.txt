cmake_minimum_required(VERSION 3.21)
project(droid-emulator VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# 6.4 是 Ubuntu 24.04 自带的版本。
find_package(Qt6 6.4 REQUIRED COMPONENTS Quick WaylandCompositor)
qt_standard_project_setup()
if(QT_KNOWN_POLICY_QTP0001)
    qt_policy(SET QTP0001 NEW)
endif()
if(QT_KNOWN_POLICY_QTP0004)
    qt_policy(SET QTP0004 NEW)
endif()

qt_add_executable(droid-emulator
    src/main.cpp
    src/backend.cpp src/backend.h
    src/androidscreen.cpp src/androidscreen.h
)

qt_add_qml_module(droid-emulator
    URI Droid
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/EmulatorWindow.qml
        qml/TitleBar.qml
        qml/SideBar.qml
        qml/IconButton.qml
        qml/Toast.qml
        qml/ResizeHandle.qml
        qml/Icons.js
)

# 开发时默认用仓库里编好的 droid 命令行，可以用环境变量 DROID_CLI 换成别的路径。
target_compile_definitions(droid-emulator PRIVATE
    DROID_DEFAULT_CLI="${CMAKE_CURRENT_SOURCE_DIR}/../target/debug/droid")

# 生成的 QML 类型注册代码按文件名包含头文件。
target_include_directories(droid-emulator PRIVATE src)
target_link_libraries(droid-emulator PRIVATE Qt6::Quick Qt6::WaylandCompositor)
