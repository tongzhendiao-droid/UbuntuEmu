import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Window
import Droid

// 模拟器窗口：标题栏、按比例缩放的安卓屏幕、右侧工具栏。
Window {
    id: win

    property var output
    property string socketName
    // 安卓屏幕的分辨率（竖屏手机），旋转时互换宽高。
    property int androidWidth: 720
    property int androidHeight: 1280
    property var toplevel: null
    property bool closingDown: false
    readonly property bool hasScreen: androidView.shellSurface !== null
    readonly property bool fullScreen: visibility === Window.FullScreen
    readonly property int titleHeight: fullScreen ? 0 : 40
    readonly property int sideWidth: 56

    visible: true
    title: "Droid"
    color: "#141518"
    flags: Qt.Window | Qt.FramelessWindowHint
    minimumWidth: 300
    minimumHeight: 360

    // Waydroid 的窗口连上来了。
    function attach(newToplevel, xdgSurface) {
        if (hasScreen) {
            console.warn("又收到一个安卓窗口，已忽略：模拟器窗口要求关闭多窗口模式")
            return
        }
        toplevel = newToplevel
        androidView.shellSurface = xdgSurface
        androidView.takeFocus()
        Backend.markRunning()
        selfTest.begin()
    }

    // 按安卓屏幕的比例调整窗口大小，最多占桌面的九成。
    function fitWindow() {
        const maxWidth = Screen.desktopAvailableWidth * 0.9 - sideWidth
        const maxHeight = Screen.desktopAvailableHeight * 0.88 - titleHeight
        const scale = Math.min(maxWidth / androidWidth, maxHeight / androidHeight, 1)
        width = Math.round(androidWidth * scale) + sideWidth
        height = Math.round(androidHeight * scale) + titleHeight
    }

    // 像转动手机一样：安卓屏幕宽高互换（Android 按新尺寸重新布局），窗口里的显示比例保持不变。
    // 不能重新按桌面大小适配窗口，否则横屏会被放大，图标显得特别大、画面也发虚。
    function rotate() {
        const shownWidth = androidView.width * androidView.scale
        const shownHeight = androidView.height * androidView.scale
        const newWidth = androidHeight
        androidHeight = androidWidth
        androidWidth = newWidth
        Backend.setOutputMode(output, androidWidth, androidHeight)
        if (toplevel)
            toplevel.sendMaximized(Qt.size(androidWidth, androidHeight))
        if (fullScreen)
            return
        const fits = shownHeight + sideWidth <= Screen.desktopAvailableWidth
                     && shownWidth + titleHeight <= Screen.desktopAvailableHeight
        if (fits) {
            width = Math.round(shownHeight) + sideWidth
            height = Math.round(shownWidth) + titleHeight
        } else {
            fitWindow()
        }
    }

    function screenshot() {
        if (!hasScreen)
            return
        const path = Backend.newScreenshotPath()
        androidView.grabToImage(result => {
            toast.show(result.saveToFile(path) ? "截图已保存到 " + path : "截图保存失败")
        }, Qt.size(androidWidth, androidHeight))
    }

    function perform(action) {
        switch (action) {
        case "back": androidView.pressKey(158); break        // KEY_BACK
        case "home": androidView.pressKey(172); break        // KEY_HOMEPAGE
        // Waydroid 只转发 0–238 的键码，多任务键本来的 580 会被丢掉，改发 F24，
        // 由 scripts/install-keylayout.sh 装的按键布局映射成 APP_SWITCH。
        case "recents": androidView.pressKey(194); break     // KEY_F24
        case "volumeUp": androidView.pressKey(115); break    // KEY_VOLUMEUP
        case "volumeDown": androidView.pressKey(114); break  // KEY_VOLUMEDOWN
        case "rotate": rotate(); break
        case "screenshot": screenshot(); break
        case "install": apkDialog.open(); break
        case "fullscreen": fullScreen ? showNormal() : showFullScreen(); break
        }
    }

    Component.onCompleted: {
        fitWindow()
        Backend.startAndroid(socketName)
    }

    // 关窗口就停掉 Android（和 MuMu 一样），停完再退出。
    onClosing: close => {
        if (closingDown)
            return
        close.accepted = false
        closingDown = true
        Backend.shutdown()
    }

    Connections {
        target: Backend
        function onShutdownFinished() { Qt.quit() }
        function onNotify(text) {
            console.log("提示", text)
            toast.show(text)
            selfTest.installFinished(text)
        }
    }

    TitleBar {
        id: titleBar
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: win.titleHeight
        visible: !win.fullScreen
        window: win
        status: {
            switch (Backend.phase) {
            case "running": return Backend.ipAddress ? "运行中 · " + Backend.ipAddress : "运行中"
            case "booting": return "正在启动"
            case "checking":
            case "switching": return "准备中"
            case "stopping": return "正在停止"
            case "stopped": return "已停止"
            case "error": return "出错了"
            default: return ""
            }
        }
    }

    Item {
        id: screenArea
        anchors { left: parent.left; top: titleBar.bottom; right: sideBar.left; bottom: parent.bottom }
        clip: true

        AndroidScreen {
            id: androidView

            // 控件保持安卓屏幕的原始大小，再整体缩放：Waydroid 把画面放在子表面里，
            // 只有整体缩放才能让子表面跟着缩放。
            displaySize: Qt.size(win.androidWidth, win.androidHeight)
            width: win.androidWidth * pixelSize
            height: win.androidHeight * pixelSize
            transformOrigin: Item.TopLeft
            scale: Math.min(screenArea.width / width, screenArea.height / height)
            x: Math.round((screenArea.width - width * scale) / 2)
            y: Math.round((screenArea.height - height * scale) / 2)
            visible: win.hasScreen
            // 最大化时 XDG 集成会挪动 moveItem，给它一个替身，屏幕的位置由我们自己排版。
            moveItem: Item {}
            onSurfaceDestroyed: {
                shellSurface = null
                win.toplevel = null
            }
        }

        Column {
            anchors.centerIn: parent
            width: Math.min(parent.width - 48, 320)
            spacing: 16
            visible: !win.hasScreen

            BusyIndicator {
                anchors.horizontalCenter: parent.horizontalCenter
                running: ["checking", "switching", "booting", "stopping"].includes(Backend.phase)
                visible: running
            }
            Text {
                width: parent.width
                text: Backend.message
                color: "#c9cacf"
                font.pixelSize: 15
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
            }
            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "重新启动 Android"
                visible: Backend.phase === "stopped" || Backend.phase === "error"
                onClicked: Backend.startAndroid(win.socketName)
            }
        }

        DropArea {
            anchors.fill: parent
            onDropped: drop => Backend.installApks(drop.urls)
        }

        Toast {
            id: toast
        }
    }

    SideBar {
        id: sideBar
        anchors { right: parent.right; top: titleBar.bottom; bottom: parent.bottom }
        width: win.sideWidth
        fullScreen: win.fullScreen
        onAction: name => win.perform(name)
    }

    FileDialog {
        id: apkDialog
        title: "选择要安装的 APK"
        fileMode: FileDialog.OpenFiles
        nameFilters: ["安卓安装包 (*.apk)"]
        onAccepted: Backend.installApks(selectedFiles)
    }

    // 窗口没有系统边框，四周留出拖动改大小的区域。
    ResizeHandle { window: win; edges: Qt.LeftEdge; cursor: Qt.SizeHorCursor; width: 5
        anchors { left: parent.left; top: parent.top; bottom: parent.bottom; topMargin: 10; bottomMargin: 10 } }
    ResizeHandle { window: win; edges: Qt.RightEdge; cursor: Qt.SizeHorCursor; width: 5
        anchors { right: parent.right; top: parent.top; bottom: parent.bottom; topMargin: 10; bottomMargin: 10 } }
    ResizeHandle { window: win; edges: Qt.BottomEdge; cursor: Qt.SizeVerCursor; height: 5
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: 10; rightMargin: 10 } }
    ResizeHandle { window: win; edges: Qt.BottomEdge | Qt.LeftEdge; cursor: Qt.SizeBDiagCursor; width: 10; height: 10
        anchors { left: parent.left; bottom: parent.bottom } }
    ResizeHandle { window: win; edges: Qt.BottomEdge | Qt.RightEdge; cursor: Qt.SizeFDiagCursor; width: 10; height: 10
        anchors { right: parent.right; bottom: parent.bottom } }

    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.color: "#2e2f34"
        visible: !win.fullScreen
    }

    // 开发用：设置 DROID_SNAPSHOT_DIR 后，安卓画面出现时自动走一遍主要操作并截图，最后关闭窗口。
    Timer {
        id: selfTest

        readonly property string dir: Backend.environment("DROID_SNAPSHOT_DIR")
        // 可选：DROID_TEST_APK 指向一个 APK 时，先走界面的安装流程（和拖放 APK 一样）。
        readonly property string apk: Backend.environment("DROID_TEST_APK")
        property bool waitingInstall: false
        property int step: 0

        function installFinished(text) {
            if (!waitingInstall || text.startsWith("正在安装"))
                return
            waitingInstall = false
            next(1500)
        }

        // 每次安卓窗口连上来都从头开始（关多窗口模式时 Android 会重启一次）。
        function begin() {
            if (dir === "")
                return
            step = 0
            interval = 25000 // 等 Android 启动完
            restart()
        }
        // 截图是异步的，要等它完成再做下一个操作，所以每次截图单独占一步。
        function save(name) {
            console.log("截图", name, "控件", androidView.width + "x" + androidView.height,
                        "像素", androidView.pixelSize.toFixed(3), "缩放", androidView.scale.toFixed(3))
            const started = win.contentItem.grabToImage(result => {
                if (!result.saveToFile(dir + "/" + name))
                    console.warn("截图保存失败", name)
            })
            if (!started)
                console.warn("截图没有开始", name)
        }
        function tap(x, y) {
            androidView.touch(0, Qt.point(x, y))
            androidView.touch(2, Qt.point(x, y))
        }
        function next(delay) {
            interval = delay
            restart()
        }

        onTriggered: {
            switch (step++) {
            case 0: win.perform("home"); next(2500); break // 先关掉开机后可能弹出的对话框
            case 1: save("1-home.png"); next(800); break
            case 2:
                if (apk === "") {
                    next(10)
                } else {
                    waitingInstall = true
                    Backend.installApks(["file://" + apk])
                    next(180000) // 正常情况下装完会由 installFinished 提前进入下一步
                }
                break
            case 3: waitingInstall = false; save("2-installed.png"); next(800); break
            case 4: swipeUp.begin(); burst.begin(); next(3000); break
            case 5: save("3-drawer.png"); next(800); break
            // 应用抽屉第 2 行第 3 个（装了 F-Droid 时是它），否则点第 3 行第 3 个
            case 6: tap(445, apk === "" ? 800 : 526); next(4000); break
            case 7: save("4-app.png"); next(800); break
            case 8: win.perform("back"); next(2500); break
            case 9: save("5-back.png"); next(800); break
            case 10: win.perform("recents"); next(3000); break
            case 11: save("6-recents.png"); next(800); break
            case 12: win.perform("home"); next(2500); break
            case 13: win.perform("rotate"); next(8000); break
            case 14: save("7-landscape.png"); next(800); break
            case 15: win.perform("rotate"); next(6000); break
            case 16: save("8-portrait.png"); win.screenshot(); next(2000); break
            default: console.log("自测完成"); win.close()
            }
        }
    }

    // 自测用：上滑动画期间连拍几帧，检查有没有黑帧或撕裂。
    Timer {
        id: burst

        property int shot: 0

        function begin() {
            shot = 0
            restart()
        }

        interval: 80
        repeat: true
        onTriggered: {
            selfTest.save("burst-" + shot + ".png")
            if (++shot >= 6)
                stop()
        }
    }

    // 自测用：从屏幕下方往上滑，打开应用抽屉。
    Timer {
        id: swipeUp

        property int tick: 0
        readonly property int steps: 15

        function begin() {
            tick = 0
            androidView.touch(0, Qt.point(win.androidWidth / 2, win.androidHeight * 0.85))
            start()
        }

        interval: 16
        repeat: true
        onTriggered: {
            tick++
            const y = win.androidHeight * (0.85 - 0.5 * Math.min(tick / steps, 1))
            androidView.touch(tick <= steps ? 1 : 2, Qt.point(win.androidWidth / 2, y))
            if (tick > steps)
                stop()
        }
    }
}
