#!/bin/sh
# 给 Waydroid 的虚拟键盘（wayland_keyboard）装一份按键布局，把 F24 映射成「多任务」键。
#
# Waydroid 的显示层只转发 0–238 的键码，多任务键本来的键码 580 会被丢掉
# （logcat 里是 "hwcomposer: Invalid key: 580"）。模拟器窗口的「多任务」按钮改发 F24（194），
# 这份布局让 Android 把它当成 APP_SWITCH，其余按键沿用镜像自带的 Generic.kl。
# 文件放在 Waydroid 的 overlay 目录里，会叠加到系统镜像上，不改动镜像本身。
#
# 用法：sudo scripts/install-keylayout.sh，装好后重启 Android 会话生效。
set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo "需要管理员权限：sudo $0" >&2
    exit 1
fi

images=$(sed -n 's/^images_path *= *//p' /var/lib/waydroid/waydroid.cfg)
images=${images:-/var/lib/waydroid/images}
target_dir=/var/lib/waydroid/overlay/system/usr/keylayout
mnt=$(mktemp -d)
trap 'umount "$mnt" 2>/dev/null || true; rmdir "$mnt"' EXIT

mount -o ro,loop "$images/system.img" "$mnt"
generic="$mnt/system/usr/keylayout/Generic.kl"
if [ ! -f "$generic" ]; then
    echo "在系统镜像里找不到 Generic.kl" >&2
    exit 1
fi

mkdir -p "$target_dir"
{
    echo "# 由 droid 的 scripts/install-keylayout.sh 生成：镜像自带的 Generic.kl，外加 F24 → APP_SWITCH。"
    grep -vE '^key 194 ' "$generic"
    echo "key 194   APP_SWITCH"
} > "$target_dir/wayland_keyboard.kl"
chmod 644 "$target_dir/wayland_keyboard.kl"
echo "已写入 $target_dir/wayland_keyboard.kl，重启 Android 会话后生效。"
