// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "kassensturz-ui-bridge/cpp/local_only.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QEventLoop>
#include <QtCore/QTimer>
#include <QtCore/QUrl>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkReply>
#include <QtNetwork/QNetworkRequest>
#include <QtQml/QQmlApplicationEngine>
#include <QtQml/QQmlNetworkAccessManagerFactory>

namespace kassensturz::network_guard
{
namespace
{
// Only what is on this computer or inside the app.
bool isLocal(const QUrl &url)
{
    const QString scheme = url.scheme().toLower();
    return url.isLocalFile() || scheme == QLatin1String("qrc") || scheme == QLatin1String("data");
}

class LocalOnlyAccessManager : public QNetworkAccessManager
{
public:
    using QNetworkAccessManager::QNetworkAccessManager;

    bool refused = false;

protected:
    QNetworkReply *createRequest(Operation operation, const QNetworkRequest &request, QIODevice *body) override
    {
        if (isLocal(request.url())) {
            return QNetworkAccessManager::createRequest(operation, request, body);
        }
        refused = true;
        // A scheme Qt has no handler for: the reply fails at once, and
        // nothing is sent anywhere.
        return QNetworkAccessManager::createRequest(GetOperation, QNetworkRequest(QUrl(QStringLiteral("kassensturz-refused:"))), nullptr);
    }
};

class Factory : public QQmlNetworkAccessManagerFactory
{
public:
    QNetworkAccessManager *create(QObject *parent) override
    {
        return new LocalOnlyAccessManager(parent);
    }
};
}

void installLocalOnly(QQmlApplicationEngine &engine)
{
    // The engine doesn't own the factory; it lives as long as the app.
    static Factory factory;
    engine.setNetworkAccessManagerFactory(&factory);
}

bool isRefused(const QString &url)
{
    if (QCoreApplication::instance() == nullptr) {
        static int argc = 1;
        static char name[] = "kassensturz-test";
        static char *argv[] = {name, nullptr};
        // Never deleted: Qt's own objects would outlive it at exit.
        new QCoreApplication(argc, argv);
    }
    LocalOnlyAccessManager manager;
    QNetworkReply *reply = manager.get(QNetworkRequest(QUrl(url)));
    if (!reply->isFinished()) {
        QEventLoop loop;
        QObject::connect(reply, &QNetworkReply::finished, &loop, &QEventLoop::quit);
        QTimer::singleShot(3000, &loop, &QEventLoop::quit);
        loop.exec();
    }
    reply->deleteLater();
    return manager.refused;
}
}
