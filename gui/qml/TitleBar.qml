import QtQuick
import "Icons.js" as Icons

// 自绘的标题栏：拖动移动窗口，右边是最小化和关闭。
Rectangle {
    id: bar

    property var window
    property string status

    color: "#1c1d20"

    DragHandler {
        target: null
        onActiveChanged: if (active) bar.window.startSystemMove()
    }

    Row {
        anchors { left: parent.left; leftMargin: 14; verticalCenter: parent.verticalCenter }
        spacing: 10

        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 20
            height: 20
            radius: 6
            color: "#e95420"
            Text {
                anchors.centerIn: parent
                text: "D"
                color: "white"
                font.bold: true
                font.pixelSize: 13
            }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "Droid"
            color: "#ececef"
            font.pixelSize: 14
            font.bold: true
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, bar.width - 200)
            text: bar.status
            color: "#8d8f96"
            font.pixelSize: 12
            elide: Text.ElideRight
        }
    }

    Row {
        anchors { right: parent.right; rightMargin: 4; verticalCenter: parent.verticalCenter }
        spacing: 2

        IconButton {
            size: 32
            iconSize: 18
            path: Icons.fullscreen
            tip: "全屏"
            onClicked: bar.window.perform("fullscreen")
        }
        IconButton {
            size: 32
            iconSize: 18
            path: Icons.minimize
            tip: "最小化"
            onClicked: bar.window.showMinimized()
        }
        IconButton {
            size: 32
            iconSize: 18
            path: Icons.close
            tip: "关闭（会停止 Android）"
            hoverColor: "#c42b1c"
            onClicked: bar.window.close()
        }
    }
}
