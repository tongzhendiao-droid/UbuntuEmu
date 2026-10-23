#include "androidscreen.h"

#include <QHoverEvent>
#include <QLoggingCategory>
#include <QMouseEvent>
#include <QPointer>
#include <QQuickWindow>
#include <QTimer>
#include <QtWaylandCompositor/QWaylandClient>
#include <QtWaylandCompositor/QWaylandCompositor>
#include <QtWaylandCompositor/QWaylandKeyboard>
#include <QtWaylandCompositor/QWaylandOutput>
#include <QtWaylandCompositor/QWaylandSeat>
#include <QtWaylandCompositor/QWaylandSurface>
#include <QtWaylandCompositor/QWaylandView>

// 调试输入问题时打开：QT_LOGGING_RULES="droid.screen.debug=true"
Q_LOGGING_CATEGORY(lcScreen, "droid.screen", QtWarningMsg)

namespace {

// Qt 的原生扫描码比 Wayland 用的 evdev 键码大 8（XKB 的历史约定），发送前要加上。
constexpr uint kXkbKeycodeOffset = 8;

enum TouchState { TouchDown = 0, TouchMove = 1, TouchUp = 2 };

} // namespace

AndroidScreen::AndroidScreen(QQuickItem *parent)
    : QWaylandQuickShellSurfaceItem(parent)
{
}

void AndroidScreen::setMouseAsTouch(bool enabled)
{
    if (enabled == m_mouseAsTouch)
        return;
    m_mouseAsTouch = enabled;
    emit mouseAsTouchChanged();
}

void AndroidScreen::setDisplaySize(const QSize &size)
{
    if (size == m_displaySize)
        return;
    m_displaySize = size;
    emit displaySizeChanged();
}

qreal AndroidScreen::pixelSize() const
{
    const qreal outputScale = (view() && view()->output()) ? view()->output()->scaleFactor() : 1;
    const qreal devicePixelRatio = window() ? window()->devicePixelRatio() : 1;
    return outputScale / devicePixelRatio;
}

void AndroidScreen::itemChange(ItemChange change, const ItemChangeData &data)
{
    QWaylandQuickShellSurfaceItem::itemChange(change, data);
    if (change == ItemSceneChange || change == ItemDevicePixelRatioHasChanged)
        emit pixelSizeChanged();
}

void AndroidScreen::pressKey(int evdevCode, int holdMs)
{
    QWaylandSurface *target = surface();
    if (!target || !compositor())
        return;
    QWaylandSeat *seat = compositor()->defaultSeat();
    seat->setKeyboardFocus(target);
    qCDebug(lcScreen) << "key" << evdevCode << "hold" << holdMs << "focus" << seat->keyboardFocus()
                      << "client" << (seat->keyboard() ? seat->keyboard()->focusClient() : nullptr);
    const uint code = uint(evdevCode) + kXkbKeycodeOffset;
    seat->sendKeyPressEvent(code);
    if (holdMs <= 0) {
        seat->sendKeyReleaseEvent(code);
        return;
    }
    QPointer<QWaylandSeat> guard(seat);
    QTimer::singleShot(holdMs, this, [guard, code] {
        if (guard)
            guard->sendKeyReleaseEvent(code);
    });
}

void AndroidScreen::touch(int state, const QPointF &surfacePosition)
{
    QWaylandSurface *target = surface();
    if (!target || !compositor())
        return;
    QWaylandSeat *seat = compositor()->defaultSeat();
    switch (state) {
    case TouchDown:
        seat->sendTouchPointPressed(target, 0, surfacePosition);
        break;
    case TouchMove:
        seat->sendTouchPointMoved(target, 0, surfacePosition);
        break;
    default:
        seat->sendTouchPointReleased(target, 0, surfacePosition);
        break;
    }
    seat->sendTouchFrameEvent(target->client());
}

QPointF AndroidScreen::toSurface(const QPointF &itemPosition) const
{
    if (m_displaySize.isEmpty() || width() <= 0 || height() <= 0)
        return mapToSurface(itemPosition);
    return QPointF(itemPosition.x() * m_displaySize.width() / width(),
                   itemPosition.y() * m_displaySize.height() / height());
}

void AndroidScreen::movePointer(const QPointF &itemPosition)
{
    if (!surface() || !compositor())
        return;
    compositor()->defaultSeat()->sendMouseMoveEvent(view(), toSurface(itemPosition), mapToScene(itemPosition));
}

void AndroidScreen::mousePressEvent(QMouseEvent *event)
{
    if (!surface()) {
        QWaylandQuickShellSurfaceItem::mousePressEvent(event);
        return;
    }
    // 键盘输入也交给 Android。
    takeFocus();
    if (m_mouseAsTouch && event->button() == Qt::LeftButton) {
        m_touching = true;
        touch(TouchDown, toSurface(event->position()));
    } else {
        movePointer(event->position());
        compositor()->defaultSeat()->sendMousePressEvent(event->button());
    }
    event->accept();
}

void AndroidScreen::mouseMoveEvent(QMouseEvent *event)
{
    if (m_touching)
        touch(TouchMove, toSurface(event->position()));
    else
        movePointer(event->position());
    event->accept();
}

void AndroidScreen::mouseReleaseEvent(QMouseEvent *event)
{
    if (m_touching && event->button() == Qt::LeftButton) {
        m_touching = false;
        touch(TouchUp, toSurface(event->position()));
    } else if (surface() && compositor()) {
        movePointer(event->position());
        compositor()->defaultSeat()->sendMouseReleaseEvent(event->button());
    }
    event->accept();
}

void AndroidScreen::hoverMoveEvent(QHoverEvent *event)
{
    movePointer(event->position());
    event->accept();
}
