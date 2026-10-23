# Droid（暂定名）

基于 Waydroid 的 Ubuntu 安卓运行时，规划见 [开发计划.md](开发计划.md)。

现在由三部分组成：

| 目录 | 内容 |
| --- | --- |
| `gui/` | 模拟器窗口（Qt 6 C++/QML）。程序内置一个 Wayland 合成器，安卓画面显示在自己的窗口里，交互参考 MuMu |
| `crates/droid-core` | 核心库（Rust）：Waydroid 交互、应用列表、环境诊断 |
| `crates/droid-cli` | `droid` 命令行（Rust），支持 `--json`，模拟器窗口也通过它操作 Waydroid |
| `crates/droid-gtk` | 早期的 GTK4 管理界面，已被模拟器窗口取代 |

## 模拟器窗口

- 安卓屏幕按 720×1280、320 dpi 的手机布局显示，窗口里按比例缩放
- 右侧工具栏：返回、主页、多任务、音量加减、旋转屏幕、截图（存到「图片/Droid」）、安装 APK；全屏按钮在标题栏上
- 旋转时像转动手机一样只互换窗口宽高，显示比例不变
- 鼠标左键当手指用，滚轮、右键、键盘照常转发
- 把 APK 拖进窗口即可安装
- 安卓应用的图标默认不出现在 Ubuntu 的应用启动器里，只在模拟器里用。模拟器运行时会立刻隐藏新装应用的图标；想恢复显示用 `droid launcher show`
- 打开窗口时启动 Android，如果会话已经开在桌面上，会先停掉再切进窗口；关掉窗口会停止 Android
- 如果 Waydroid 开着多窗口模式，启动时会自动关掉，Android 会因此重启一次。多窗口模式下应用会以小窗口开在安卓桌面里，画面也会闪
- 「多任务」按钮需要先运行一次 `sudo scripts/install-keylayout.sh`（原因见下文的已知问题），然后重启 Android

编译运行（需要 Wayland 会话和已经 `waydroid init` 过的 Waydroid）：

```bash
sudo apt install qt6-base-dev qt6-declarative-dev qt6-wayland-dev qml6-module-qtwayland-compositor qml6-module-qtquick-controls qml6-module-qtquick-dialogs qml6-module-qtquick-shapes
```

```bash
cargo build -p droid-cli
```

```bash
cmake -S gui -B gui/build -G Ninja && cmake --build gui/build
```

```bash
./gui/build/droid-emulator
```

窗口默认使用 `target/debug/droid`，可以用环境变量 `DROID_CLI` 指定别的路径。

### 实现方式

- 程序本身是一个 Wayland 合成器（`$XDG_RUNTIME_DIR/droid-0`）。启动会话时把 `WAYLAND_DISPLAY` 指向它，Waydroid 的画面就连到这里而不是桌面。
- 合成器输出的分辨率就是安卓屏幕的分辨率；`GRID_UNIT_PX=16` 让 Waydroid 把屏幕密度设成 320 dpi。
- Waydroid 把画面放在子表面里，所以控件保持安卓的原始尺寸再整体缩放。旋转时只有子表面会变大小，排版和输入坐标都按我们设定的分辨率计算，不依赖顶层表面的尺寸。
- Waydroid 给每个安卓应用生成 `~/.local/share/applications/waydroid.<包名>.desktop`。隐藏时给它加上 `NoDisplay=true` 和标记 `X-Droid-Hidden=true`，恢复时只撤销带标记的；系统应用本来就被 Waydroid 隐藏，不受影响。Waydroid 重写这些文件时会保留 `NoDisplay`，所以只需要处理新出现的：模拟器监视这个目录，`droid app install` 装完也会处理一次。设置保存在 `~/.config/droid/config.json`。
- 返回、主页、音量用 evdev 键码（158、172、115、114）发给 Android，对应 `Generic.kl` 里的映射。多任务改发 F24（194），由 `scripts/install-keylayout.sh` 映射成 APP_SWITCH。

## 命令行

| 命令 | 作用 |
| --- | --- |
| `droid status` | Waydroid 和会话的完整状态 |
| `droid doctor` | 环境诊断 |
| `droid app list [--live]` | 已安装的应用；`--live` 直接问运行中的 Android |
| `droid app launch <包名>` | 打开应用，会话没运行时自动启动 |
| `droid app install <apk>` | 安装 APK，会话没运行时自动启动并等 Android 启动完成 |
| `droid app remove <包名>` | 卸载应用；Waydroid 不能直接卸载时，会在安卓里打开卸载确认框 |
| `droid launcher [hide\|show\|sync]` | 安卓应用图标是否出现在 Ubuntu 应用启动器里（默认 hide）；`sync` 按当前设置处理新出现的图标 |
| `droid session start [--wait]` | 后台启动会话；`--wait` 等到 Android 启动完成 |
| `droid session stop` | 停止会话 |
| `droid session status` | 只查会话是否运行、显示接口和 IP，适合轮询 |
| `droid multi-window [on\|off]` | 查看或切换多窗口模式，重启会话后生效 |
| `droid full-ui` | 打开完整的安卓界面 |

加 `--json` 时，成功输出 `{"ok":true,"data":…}`，失败输出 `{"ok":false,"error":{"code":…,"message":…}}`。错误码和退出码是稳定接口，只增不改：

| 退出码 | code | 含义 |
| --- | --- | --- |
| 0 | | 成功 |
| 1 | `INTERNAL` | 内部错误 |
| 2 | | 命令行参数错误 |
| 3 | `NOT_INSTALLED` | 没有安装 Waydroid |
| 4 | `NOT_INITIALIZED` | 没有执行 `waydroid init` |
| 5 | `SESSION_NOT_RUNNING` | 需要 Android 会话在运行 |
| 6 | `CONTAINER_UNAVAILABLE` | 连接不到 Waydroid 容器服务 |
| 7 | `INVALID_ARGUMENT` | 参数不合法 |
| 8 | `TIMEOUT` | 超时 |
| 9 | `COMMAND_FAILED` | waydroid 命令执行失败 |

## Waydroid 1.6.3 的已知问题

- **容器服务泄漏文件句柄。** 会话运行时，每次调用容器服务的 `GetSession`（`waydroid status`、`prop`、`app` 等命令内部都会调用）泄漏 2 个句柄；每启停一次会话也会泄漏几十个。默认上限是 1024，用完之后会话启动失败，日志里是 `Too many open files`。所以 `droid` 轮询时不走 `GetSession`，改看 session bus 上的会话进程。根本的办法是调高上限：`sudo systemctl edit waydroid-container`，加上 `[Service]` 和 `LimitNOFILE=65536` 两行，然后重启这个服务。`droid doctor` 会检查这一项。
- **显示层只转发 0–238 的键码。** 更大的键码会被丢掉，logcat 里能看到 `hwcomposer: Invalid key: 580`，所以多任务键（580）发不过去。`scripts/install-keylayout.sh` 会在 Waydroid 的 overlay 目录里放一份 `wayland_keyboard.kl`：内容是镜像自带的 `Generic.kl`，外加 `key 194 APP_SWITCH`。这份文件叠加在系统镜像上，不改动镜像本身。
- **「旋转」其实是改屏幕尺寸。** Waydroid 只能通过改窗口尺寸让 Android 切换横竖屏，Android 会把横屏当成天生横屏的设备，桌面上的导航栏会压在底部图标上（应用里不受影响）。要像真手机一样旋转，需要在 Android 里设置屏幕方向，再把画面和触摸坐标转 90°。
- **多窗口模式下画面会闪。** 这个模式下 Waydroid 会把每个安卓图层拆成子表面，图层布局一变就会挂空缓冲区、清空缓存，所以模拟器窗口只用单窗口（完整界面）模式。
- **`waydroid shell` 会把标准输出指向的文件改成 root 所有、权限 600。** 要用管道接它的输出，不要直接重定向到自己的文件。
- **静默卸载会失败。** `waydroid app remove` 报 `Failed with code: -3`（Android 的 `DELETE_FAILED_USER_RESTRICTED`），用 root 执行 `pm uninstall` 则可以。`droid app remove` 遇到这种情况会改为打开系统的卸载确认框。
- `waydroid` 出错时退出码通常还是 0，只在 stderr 打日志，要按 stderr 的内容判断结果。
- 容器进入运行状态后，Android 还要十几秒才能启动完成（`sys.boot_completed=1`）。这之前查到的应用列表和 IP 都不完整。

## 开发说明

- Ubuntu 打包的 Qt 没有 linux-dmabuf 插件，Waydroid 会退回用共享内存传画面，多一次拷贝。
- Qt Wayland Compositor 只有 GPLv3 和商业授权，所以模拟器窗口这个进程要按 GPLv3 发布，或者购买商业授权。核心库和命令行是独立进程，不受影响。
- 模拟器自测：`DROID_SNAPSHOT_DIR=/tmp/shots ./gui/build/droid-emulator`。安卓画面出现后，会依次测试主页、上滑（同时连拍几帧检查闪烁）、打开应用、返回、多任务、旋转和截图，每步截一张图，最后关闭窗口。再加上 `DROID_TEST_APK=/路径/某个.apk`，会先走一遍界面的安装流程（和拖放 APK 一样）。
- 输入调试日志：`QT_LOGGING_RULES="droid.screen.debug=true"`。
- 核心库单元测试：`cargo test -p droid-core`。

## 分发时怎么处理系统配置

开发时有几项是手动在本机上改的，发布时要收进安装包或首次运行的流程里：

| 开发时的做法 | 发布时的做法 |
| --- | --- |
| 手动在 `/etc/systemd/system/waydroid-container.service.d/` 加 `LimitNOFILE=65536` | 安装包带一个 systemd drop-in，放在 `/usr/lib/systemd/system/waydroid-container.service.d/`（`/etc` 留给管理员覆盖）。postinst 执行 `systemctl daemon-reload`，卸载时 dpkg 会删掉它。同时向 Waydroid 上游报告句柄泄漏 |
| `sudo scripts/install-keylayout.sh` 生成多任务按键映射 | 安装 .deb 时还不能生成：那时可能还没下载安卓镜像，而文件内容要从镜像里取。改成首次运行或环境诊断里的「一键修复」，通过 pkexec 加 polkit 策略调用一个只做白名单操作的小助手，用户输入一次密码。长期办法是给 Waydroid 提 PR，把只收 0–238 键码的限制放宽，就不再需要这个文件 |
| 关闭多窗口模式、设置屏幕密度 | 程序自己处理，不需要管理员权限 |
| 安装 GTK、Qt 的开发包 | 只有编译时需要。运行时依赖写进 .deb：`Depends` 写 waydroid 和 Qt 的 QML、合成器模块，`Recommends` 写 adb、wl-clipboard |
| 用 `target/debug/droid` | `droid` 和 `droid-emulator` 一起装到 `/usr/bin`，界面会先找自己旁边的 `droid` |
| 应用 ID `com.example.Droid`，没有桌面入口 | 定名后替换，安装包带 .desktop 文件和图标 |
| Waydroid 为每个应用生成的桌面图标会绕过模拟器，直接在桌面上打开 | 程序改写这些图标的启动命令，让它们在模拟器窗口里打开 |
| 模拟器用了 Qt Wayland Compositor（GPLv3） | 模拟器按 GPLv3 开源发布，和免费版开源的计划一致；Pro 的闭源功能放在独立进程里 |
| 只在 Ubuntu 26.04 上验证过 | PPA 为 24.04 和 26.04 分别构建；24.04 自带的 Qt 6.4 需要实际编译验证 |

## 还没做

- 中文输入直通（P0）。Waydroid 不支持桌面输入法协议，需要安卓侧的桥接输入法。
- 隐藏安卓自带的导航栏（工具栏已经有返回、主页、多任务）
- 每个安卓应用一个窗口的多窗口模式（合成器已经能拿到每个窗口，还没做界面）
- GPU 缓冲区直通（dmabuf）、多开、键位映射
- .deb 打包、桌面入口和图标；应用 ID `com.example.Droid` 是占位的
