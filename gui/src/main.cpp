// Droid 模拟器窗口：内置一个 Wayland 合成器，把 Waydroid 的安卓画面显示在自己的窗口里。

#include <QGuiApplication>
#include <QQmlApplicationEngine>

int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);
    QGuiApplication::setApplicationName(QStringLiteral("Droid"));
    QGuiApplication::setApplicationVersion(QStringLiteral("0.1.0"));
    // 占位的应用 ID，产品定名后替换。
    QGuiApplication::setDesktopFileName(QStringLiteral("com.example.Droid"));

    QQmlApplicationEngine engine;
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
#if QT_VERSION >= QT_VERSION_CHECK(6, 5, 0)
    engine.loadFromModule("Droid", "Main");
#else
    engine.load(QUrl(QStringLiteral("qrc:/Droid/qml/Main.qml")));
#endif
    return app.exec();
}
