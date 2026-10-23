import QtQuick
import QtWayland.Compositor
import QtWayland.Compositor.XdgShell
import Droid

// 内置的 Wayland 合成器。Waydroid 连到这里而不是桌面的合成器，安卓画面就显示在我们自己的模拟器窗口里。
WaylandCompositor {
    id: compositor

    // 对应 $XDG_RUNTIME_DIR/droid-0。
    socketName: "droid-0"

    WaylandOutput {
        id: output

        compositor: compositor
        // 输出的大小就是安卓屏幕的分辨率，和窗口大小无关，窗口里按比例缩放显示。
        sizeFollowsWindow: false
        window: EmulatorWindow {
            output: output
            socketName: compositor.socketName
        }
        Component.onCompleted: Backend.setOutputMode(output, window.androidWidth, window.androidHeight)
    }

    XdgShell {
        onToplevelCreated: (toplevel, xdgSurface) => output.window.attach(toplevel, xdgSurface)
    }
}
