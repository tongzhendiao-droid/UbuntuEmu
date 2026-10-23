#pragma once

#include <QFileSystemWatcher>
#include <QJsonValue>
#include <QObject>
#include <QProcessEnvironment>
#include <QStringList>
#include <QTimer>
#include <QUrl>
#include <QtQml/qqmlregistration.h>

#include <functional>

// 界面的后端：通过 droid 命令行（--json）操作 Waydroid，并管理模拟器窗口对应的 Android 会话。
// 界面不直接调用 waydroid，和命令行、以后的 MCP 用同一套逻辑。
class Backend : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON
    // checking / switching / booting / running / stopping / stopped / error
    Q_PROPERTY(QString phase READ phase NOTIFY phaseChanged)
    Q_PROPERTY(QString message READ message NOTIFY phaseChanged)
    Q_PROPERTY(QString ipAddress READ ipAddress NOTIFY ipAddressChanged)

public:
    explicit Backend(QObject *parent = nullptr);
    ~Backend() override;

    QString phase() const { return m_phase; }
    QString message() const { return m_message; }
    QString ipAddress() const { return m_ipAddress; }

    // 在指定的合成器 socket 上启动 Android 并打开完整界面；会话已经开在桌面上时先停掉再启动。
    Q_INVOKABLE void startAndroid(const QString &socketName);
    // 安卓画面已经显示出来。
    Q_INVOKABLE void markRunning();
    // 停止我们启动的会话，完成后发出 shutdownFinished。
    Q_INVOKABLE void shutdown();
    // 依次安装 APK。
    Q_INVOKABLE void installApks(const QList<QUrl> &files);
    // 设置合成器输出的分辨率，也就是安卓屏幕的分辨率。
    Q_INVOKABLE void setOutputMode(QObject *output, int width, int height);
    Q_INVOKABLE QString newScreenshotPath() const;
    Q_INVOKABLE QString environment(const QString &name) const;

signals:
    void phaseChanged();
    void ipAddressChanged();
    void notify(const QString &text);
    void shutdownFinished();

private:
    using Callback = std::function<void(bool ok, const QJsonValue &data, const QString &error)>;

    void runCli(const QStringList &args, Callback done,
                const QProcessEnvironment &env = QProcessEnvironment::systemEnvironment());
    // 运行中的会话如果开着多窗口模式就关掉，然后调用 next(changed)。
    void disableMultiWindow(std::function<void(bool changed)> next);
    void restartIntoWindow();
    void launchFullUi();
    void pollSession();
    void installNext();
    void syncLauncher();
    void setPhase(const QString &phase, const QString &message);
    QString cliPath() const;

    QString m_phase = QStringLiteral("idle");
    QString m_message;
    QString m_ipAddress;
    QString m_socketName;
    QTimer m_pollTimer;
    QFileSystemWatcher m_launcherWatcher;
    QTimer m_launcherSync;
    QStringList m_installQueue;
    bool m_polling = false;
    bool m_installing = false;
    bool m_windowModeChecked = false;
    // 会话是我们启动的，退出时要负责停掉。
    bool m_sessionOwned = false;
};
