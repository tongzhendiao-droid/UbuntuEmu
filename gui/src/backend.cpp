#include "backend.h"

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QStandardPaths>
#include <QtWaylandCompositor/QWaylandOutput>
#include <QtWaylandCompositor/QWaylandOutputMode>

namespace {

// Waydroid 用 GRID_UNIT_PX × 20 作为安卓的屏幕密度：16 对应 320 dpi，720×1280 的屏幕会按手机布局显示。
const QString kGridUnitPx = QStringLiteral("16");
constexpr int kPollIntervalMs = 2000;
constexpr int kLauncherSyncDelayMs = 300;
constexpr int kStopTimeoutMs = 30000;

bool isActive(const QString &state)
{
    return state == QLatin1String("running") || state == QLatin1String("frozen");
}

} // namespace

Backend::Backend(QObject *parent)
    : QObject(parent)
{
    m_pollTimer.setInterval(kPollIntervalMs);
    connect(&m_pollTimer, &QTimer::timeout, this, &Backend::pollSession);

    // Waydroid 在开机和装应用时会生成或重写安卓应用的启动器图标，按设置把它们藏起来，只在模拟器里用。
    // 一次改动会带来一串目录事件，合并成一次同步。
    m_launcherSync.setSingleShot(true);
    m_launcherSync.setInterval(kLauncherSyncDelayMs);
    connect(&m_launcherSync, &QTimer::timeout, this, &Backend::syncLauncher);
    const QString applications = QStandardPaths::writableLocation(QStandardPaths::ApplicationsLocation);
    if (QDir().mkpath(applications) && m_launcherWatcher.addPath(applications)) {
        connect(&m_launcherWatcher, &QFileSystemWatcher::directoryChanged, &m_launcherSync,
                qOverload<>(&QTimer::start));
    }
    syncLauncher();
}

Backend::~Backend()
{
    // 没走正常的关闭流程时也要停掉我们启动的会话：合成器一退出，安卓的显示就断了。
    if (m_sessionOwned) {
        QProcess process;
        process.start(cliPath(), {QStringLiteral("session"), QStringLiteral("stop")});
        process.waitForFinished(kStopTimeoutMs);
    }
}

void Backend::startAndroid(const QString &socketName)
{
    m_socketName = socketName;
    setPhase(QStringLiteral("checking"), tr("正在检查 Android 会话…"));
    runCli({QStringLiteral("status")}, [this](bool ok, const QJsonValue &data, const QString &error) {
        if (!ok) {
            setPhase(QStringLiteral("error"), error);
            return;
        }
        const QJsonObject status = data.toObject();
        if (!status.value(QStringLiteral("installed")).toBool()) {
            setPhase(QStringLiteral("error"), tr("没有安装 Waydroid"));
            return;
        }
        if (!status.value(QStringLiteral("initialized")).toBool()) {
            setPhase(QStringLiteral("error"), tr("Waydroid 还没有初始化，请先运行 sudo waydroid init"));
            return;
        }
        const bool onOtherDisplay = isActive(status.value(QStringLiteral("session")).toString())
            && status.value(QStringLiteral("wayland_display")).toString() != m_socketName;
        if (!onOtherDisplay) {
            launchFullUi();
            return;
        }
        // 会话开在桌面上：趁它还在运行把多窗口模式关掉（省一次重启），再停掉、在模拟器窗口里重新启动。
        setPhase(QStringLiteral("switching"), tr("正在把 Android 切换到模拟器窗口…"));
        disableMultiWindow([this](bool) {
            m_windowModeChecked = true;
            restartIntoWindow();
        });
    });
}

void Backend::disableMultiWindow(std::function<void(bool changed)> next)
{
    runCli({QStringLiteral("multi-window")}, [this, next](bool ok, const QJsonValue &data, const QString &) {
        if (!ok || !data.toObject().value(QStringLiteral("enabled")).toBool()) {
            next(false);
            return;
        }
        runCli({QStringLiteral("multi-window"), QStringLiteral("off")},
               [next](bool ok, const QJsonValue &, const QString &) { next(ok); });
    });
}

void Backend::restartIntoWindow()
{
    m_pollTimer.stop();
    runCli({QStringLiteral("session"), QStringLiteral("stop")},
           [this](bool ok, const QJsonValue &, const QString &error) {
               if (ok)
                   launchFullUi();
               else
                   setPhase(QStringLiteral("error"), error);
           });
}

void Backend::launchFullUi()
{
    setPhase(QStringLiteral("booting"), tr("正在启动 Android，第一次大约需要十几秒…"));
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    env.insert(QStringLiteral("WAYLAND_DISPLAY"), m_socketName);
    env.insert(QStringLiteral("GRID_UNIT_PX"), kGridUnitPx);
    m_sessionOwned = true;
    // droid full-ui 在后台运行 waydroid show-full-ui：会话没运行时由它启动会话。
    // 它继承这里的环境变量，所以安卓画面会连到我们的合成器上。
    runCli({QStringLiteral("full-ui")},
           [this](bool ok, const QJsonValue &, const QString &error) {
               if (!ok) {
                   m_sessionOwned = false;
                   setPhase(QStringLiteral("error"), error);
                   return;
               }
               m_pollTimer.start();
           },
           env);
}

void Backend::markRunning()
{
    setPhase(QStringLiteral("running"), QString());
    if (m_windowModeChecked)
        return;
    m_windowModeChecked = true;
    // 模拟器窗口要求单窗口（完整界面）模式。多窗口模式下 Waydroid 把每个安卓图层拆成子表面，
    // 应用会以自由窗口的形式开在安卓桌面里，画面也会闪。这个属性要重启会话才生效。
    disableMultiWindow([this](bool changed) {
        if (!changed)
            return;
        setPhase(QStringLiteral("switching"), tr("正在关闭多窗口模式，Android 会重启一次…"));
        restartIntoWindow();
    });
}

void Backend::pollSession()
{
    if (m_polling)
        return;
    m_polling = true;
    runCli({QStringLiteral("session"), QStringLiteral("status")},
           [this](bool ok, const QJsonValue &data, const QString &) {
               m_polling = false;
               if (!ok)
                   return;
               const QJsonObject session = data.toObject();
               const QString ip = session.value(QStringLiteral("ip_address")).toString();
               if (ip != m_ipAddress) {
                   m_ipAddress = ip;
                   emit ipAddressChanged();
               }
               if (m_phase == QLatin1String("running")
                   && session.value(QStringLiteral("state")).toString() == QLatin1String("stopped")) {
                   m_sessionOwned = false;
                   m_pollTimer.stop();
                   setPhase(QStringLiteral("stopped"), tr("Android 已停止"));
               }
           });
}

void Backend::shutdown()
{
    m_pollTimer.stop();
    if (!m_sessionOwned) {
        emit shutdownFinished();
        return;
    }
    setPhase(QStringLiteral("stopping"), tr("正在停止 Android…"));
    runCli({QStringLiteral("session"), QStringLiteral("stop")},
           [this](bool, const QJsonValue &, const QString &) {
               m_sessionOwned = false;
               emit shutdownFinished();
           });
}

void Backend::installApks(const QList<QUrl> &files)
{
    int queued = 0;
    for (const QUrl &url : files) {
        const QString path = url.toLocalFile();
        if (path.endsWith(QLatin1String(".apk"), Qt::CaseInsensitive)) {
            m_installQueue.append(path);
            ++queued;
        }
    }
    if (queued == 0) {
        emit notify(tr("只能安装 .apk 文件"));
        return;
    }
    installNext();
}

void Backend::installNext()
{
    // Waydroid 安装时总是把 APK 复制到同一个临时文件，所以一次只装一个。
    if (m_installing || m_installQueue.isEmpty())
        return;
    m_installing = true;
    const QString path = m_installQueue.takeFirst();
    const QString name = QFileInfo(path).fileName();
    emit notify(tr("正在安装 %1…").arg(name));
    runCli({QStringLiteral("app"), QStringLiteral("install"), path},
           [this, name](bool ok, const QJsonValue &data, const QString &error) {
               m_installing = false;
               if (!ok) {
                   emit notify(tr("%1 安装失败：%2").arg(name, error));
               } else {
                   QStringList installed;
                   const QJsonArray apps = data.toObject().value(QStringLiteral("new_apps")).toArray();
                   for (const QJsonValue &app : apps)
                       installed << app.toObject().value(QStringLiteral("name")).toString();
                   emit notify(installed.isEmpty()
                                   ? tr("%1 已执行安装，但没发现新应用（可能是更新，或 APK 只带 ARM 库）").arg(name)
                                   : tr("已安装 %1").arg(installed.join(QStringLiteral("、"))));
               }
               installNext();
           });
}

void Backend::syncLauncher()
{
    runCli({QStringLiteral("launcher"), QStringLiteral("sync")},
           [](bool ok, const QJsonValue &, const QString &error) {
               if (!ok)
                   qWarning().noquote() << "同步启动器图标失败：" << error;
           });
}

void Backend::setOutputMode(QObject *outputObject, int width, int height)
{
    auto *output = qobject_cast<QWaylandOutput *>(outputObject);
    if (!output)
        return;
    const QWaylandOutputMode mode(QSize(width, height), 60000);
    output->addMode(mode, true);
    output->setCurrentMode(mode);
}

QString Backend::newScreenshotPath() const
{
    QDir dir(QStandardPaths::writableLocation(QStandardPaths::PicturesLocation));
    dir.mkpath(QStringLiteral("Droid"));
    dir.cd(QStringLiteral("Droid"));
    const QString stamp = QDateTime::currentDateTime().toString(QStringLiteral("yyyyMMdd-HHmmss"));
    return dir.filePath(QStringLiteral("截图-%1.png").arg(stamp));
}

QString Backend::environment(const QString &name) const
{
    return qEnvironmentVariable(name.toUtf8().constData());
}

void Backend::runCli(const QStringList &args, Callback done, const QProcessEnvironment &env)
{
    auto *process = new QProcess(this);
    process->setProcessEnvironment(env);
    connect(process, &QProcess::finished, this, [process, done](int, QProcess::ExitStatus) {
        process->deleteLater();
        const QJsonObject reply = QJsonDocument::fromJson(process->readAllStandardOutput()).object();
        if (reply.value(QStringLiteral("ok")).toBool()) {
            done(true, reply.value(QStringLiteral("data")), QString());
            return;
        }
        QString message = reply.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
        if (message.isEmpty())
            message = QString::fromUtf8(process->readAllStandardError()).trimmed();
        done(false, QJsonValue(), message);
    });
    connect(process, &QProcess::errorOccurred, this, [process, done](QProcess::ProcessError error) {
        if (error != QProcess::FailedToStart)
            return;
        process->deleteLater();
        done(false, QJsonValue(), tr("找不到 droid 命令行：%1").arg(process->program()));
    });
    process->start(cliPath(), QStringList{QStringLiteral("--json")} + args);
}

void Backend::setPhase(const QString &phase, const QString &message)
{
    if (phase == m_phase && message == m_message)
        return;
    m_phase = phase;
    m_message = message;
    emit phaseChanged();
}

QString Backend::cliPath() const
{
    const QString fromEnv = qEnvironmentVariable("DROID_CLI");
    if (!fromEnv.isEmpty())
        return fromEnv;
    const QString besideApp = QCoreApplication::applicationDirPath() + QStringLiteral("/droid");
    if (QFileInfo(besideApp).isExecutable())
        return besideApp;
    if (QFileInfo(QStringLiteral(DROID_DEFAULT_CLI)).isExecutable())
        return QStringLiteral(DROID_DEFAULT_CLI);
    return QStringLiteral("droid");
}
