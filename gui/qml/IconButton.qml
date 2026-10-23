import QtQuick
import QtQuick.Controls
import QtQuick.Shapes

// 线条图标按钮。图标是 24×24 坐标系里的 SVG 路径，见 Icons.js。
Item {
    id: button

    property string path
    property string tip
    property int size: 40
    property int iconSize: 22
    property color color: "#d9dade"
    property color hoverColor: "#34363b"

    signal clicked()

    width: size
    height: size

    Rectangle {
        anchors.fill: parent
        radius: 8
        color: mouse.pressed ? Qt.darker(button.hoverColor, 1.25)
                             : mouse.containsMouse ? button.hoverColor : "transparent"
    }

    Shape {
        anchors.centerIn: parent
        width: button.iconSize
        height: button.iconSize
        layer.enabled: true
        layer.samples: 4

        ShapePath {
            scale: Qt.size(button.iconSize / 24, button.iconSize / 24)
            strokeColor: button.color
            strokeWidth: 1.8
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: button.path }
        }
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: button.clicked()
    }

    ToolTip.visible: mouse.containsMouse && tip !== ""
    ToolTip.text: tip
    ToolTip.delay: 500
}
