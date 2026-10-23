import QtQuick

// 屏幕底部的提示，几秒后自动消失。
Rectangle {
    id: toast

    function show(text) {
        label.text = text
        opacity = 1
        timer.restart()
    }

    anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 24 }
    width: Math.min(label.implicitWidth + 32, parent.width - 32)
    height: label.height + 20
    radius: 8
    color: "#e6202124"
    opacity: 0
    visible: opacity > 0

    Behavior on opacity { NumberAnimation { duration: 200 } }

    Text {
        id: label
        anchors.centerIn: parent
        width: parent.width - 32
        color: "white"
        font.pixelSize: 13
        wrapMode: Text.Wrap
        horizontalAlignment: Text.AlignHCenter
    }

    Timer {
        id: timer
        interval: 3500
        onTriggered: toast.opacity = 0
    }
}
