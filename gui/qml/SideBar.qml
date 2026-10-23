import QtQuick
import "Icons.js" as Icons

// 右侧工具栏，参考 MuMu 的侧边栏。
Rectangle {
    id: side

    property bool fullScreen: false

    signal action(string name)

    color: "#1c1d20"
    clip: true

    Rectangle {
        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
        width: 1
        color: "#2e2f34"
    }

    Column {
        anchors { top: parent.top; topMargin: 10; horizontalCenter: parent.horizontalCenter }
        spacing: 4

        IconButton { path: Icons.back; tip: "返回"; onClicked: side.action("back") }
        IconButton { path: Icons.home; tip: "主页"; onClicked: side.action("home") }
        IconButton { path: Icons.recents; tip: "多任务"; onClicked: side.action("recents") }
        Item {
            width: 40
            height: 9
            Rectangle { anchors.centerIn: parent; width: 24; height: 1; color: "#34363b" }
        }
        IconButton { path: Icons.volumeUp; tip: "音量加"; onClicked: side.action("volumeUp") }
        IconButton { path: Icons.volumeDown; tip: "音量减"; onClicked: side.action("volumeDown") }
        IconButton { path: Icons.rotate; tip: "旋转屏幕"; onClicked: side.action("rotate") }
        IconButton { path: Icons.screenshot; tip: "截图（存到 图片/Droid）"; onClicked: side.action("screenshot") }
        IconButton { path: Icons.install; tip: "安装 APK（也可以把文件拖进窗口）"; onClicked: side.action("install") }
    }

    // 平时全屏按钮在标题栏上；全屏时标题栏隐藏，退出按钮放在这里。
    IconButton {
        anchors { bottom: parent.bottom; bottomMargin: 10; horizontalCenter: parent.horizontalCenter }
        visible: side.fullScreen
        path: Icons.exitFullscreen
        tip: "退出全屏"
        onClicked: side.action("fullscreen")
    }
}
