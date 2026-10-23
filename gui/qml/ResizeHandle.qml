import QtQuick

// 无边框窗口的拖动改大小区域，交给桌面合成器处理。
Item {
    id: handle

    property var window
    property int edges
    property int cursor: Qt.ArrowCursor

    DragHandler {
        target: null
        onActiveChanged: if (active) handle.window.startSystemResize(handle.edges)
    }

    HoverHandler {
        cursorShape: handle.cursor
    }
}
