#pragma once

#include <QSize>
#include <QtQml/qqmlregistration.h>
#include <QtWaylandCompositor/QWaylandQuickShellSurfaceItem>

// 显示安卓画面的控件。鼠标左键按触摸处理（和在手机上用手指一样），滚轮、右键、键盘照常转发。
class AndroidScreen : public QWaylandQuickShellSurfaceItem
{
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(bool mouseAsTouch READ mouseAsTouch WRITE setMouseAsTouch NOTIFY mouseAsTouchChanged)
    // 安卓屏幕的分辨率（像素）。输入坐标按它换算：Waydroid 旋转后只更新放画面的子表面，
    // 顶层表面的尺寸不会跟着变，不能用它来换算。
    Q_PROPERTY(QSize displaySize READ displaySize WRITE setDisplaySize NOTIFY displaySizeChanged)
    // 安卓的 1 个像素在这里占多少逻辑像素，和合成器摆放子表面用的比例一致。
    // QML 里的 Screen.devicePixelRatio 在分数缩放下给的是整数，不能用。
    Q_PROPERTY(qreal pixelSize READ pixelSize NOTIFY pixelSizeChanged)

public:
    explicit AndroidScreen(QQuickItem *parent = nullptr);

    bool mouseAsTouch() const { return m_mouseAsTouch; }
    void setMouseAsTouch(bool enabled);
    QSize displaySize() const { return m_displaySize; }
    void setDisplaySize(const QSize &size);
    qreal pixelSize() const;

    // 按一下键（Linux evdev 键码），用于返回、主页、多任务、音量这些按钮。holdMs 是按住多久再松开。
    Q_INVOKABLE void pressKey(int evdevCode, int holdMs = 0);
    // 在安卓屏幕坐标上模拟触摸：state 为 0 按下、1 移动、2 抬起。
    Q_INVOKABLE void touch(int state, const QPointF &surfacePosition);

signals:
    void mouseAsTouchChanged();
    void displaySizeChanged();
    void pixelSizeChanged();

protected:
    void itemChange(ItemChange change, const ItemChangeData &data) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void hoverMoveEvent(QHoverEvent *event) override;

private:
    QPointF toSurface(const QPointF &itemPosition) const;
    void movePointer(const QPointF &itemPosition);

    QSize m_displaySize;
    bool m_mouseAsTouch = true;
    bool m_touching = false;
};
