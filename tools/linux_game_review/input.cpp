#include <QCoreApplication>
#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusMessage>
#include <QDBusUnixFileDescriptor>
#include <QVariant>

#include <libei.h>
#include <linux/input-event-codes.h>
#include <poll.h>
#include <unistd.h>

#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <thread>

struct Devices {
    ei_device *keyboard = nullptr;
    ei_device *pointer = nullptr;
    ei_device *scroll = nullptr;
};

static bool dispatch_until(ei *ctx, Devices &devices, int timeout_ms) {
    const auto deadline =
        std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);
    while (std::chrono::steady_clock::now() < deadline) {
        pollfd pfd{ei_get_fd(ctx), POLLIN, 0};
        poll(&pfd, 1, 50);
        ei_dispatch(ctx);
        while (ei_event *event = ei_get_event(ctx)) {
            const auto type = ei_event_get_type(event);
            if (type == EI_EVENT_SEAT_ADDED) {
                ei_seat_bind_capabilities(
                    ei_event_get_seat(event), EI_DEVICE_CAP_POINTER,
                    EI_DEVICE_CAP_KEYBOARD, EI_DEVICE_CAP_SCROLL, nullptr);
            } else if (type == EI_EVENT_DEVICE_RESUMED) {
                ei_device *device = ei_event_get_device(event);
                ei_device_start_emulating(device, 1);
                if (!devices.keyboard &&
                    ei_device_has_capability(device, EI_DEVICE_CAP_KEYBOARD)) {
                    devices.keyboard = ei_device_ref(device);
                }
                if (!devices.pointer &&
                    ei_device_has_capability(device, EI_DEVICE_CAP_POINTER)) {
                    devices.pointer = ei_device_ref(device);
                }
                if (!devices.scroll &&
                    ei_device_has_capability(device, EI_DEVICE_CAP_SCROLL)) {
                    devices.scroll = ei_device_ref(device);
                }
            } else if (type == EI_EVENT_DISCONNECT) {
                ei_event_unref(event);
                return false;
            }
            ei_event_unref(event);
        }
        if (devices.keyboard && devices.pointer && devices.scroll) {
            return true;
        }
    }
    return devices.keyboard || devices.pointer || devices.scroll;
}

static void frame(ei *ctx, ei_device *device) {
    ei_device_frame(device, ei_now(ctx));
    ei_dispatch(ctx);
    std::this_thread::sleep_for(std::chrono::milliseconds(60));
}

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    QDBusInterface remote(
        "org.kde.KWin", "/org/kde/KWin/EIS/RemoteDesktop",
        "org.kde.KWin.EIS.RemoteDesktop", QDBusConnection::sessionBus());
    QDBusMessage reply = remote.call("connectToEIS", 3);
    if (reply.type() == QDBusMessage::ErrorMessage ||
        reply.arguments().size() < 2) {
        std::fprintf(stderr, "connectToEIS failed: %s\n",
                     reply.errorMessage().toUtf8().constData());
        return 1;
    }
    const auto descriptor =
        qvariant_cast<QDBusUnixFileDescriptor>(reply.arguments().at(0));
    const int fd = dup(descriptor.fileDescriptor());
    const int cookie = reply.arguments().at(1).toInt();
    if (fd < 0) {
        std::perror("dup");
        return 1;
    }

    ei *ctx = ei_new_sender(nullptr);
    ei_configure_name(ctx, "Cuboid Wars visual review");
    if (ei_setup_backend_fd(ctx, fd) < 0) {
        std::fprintf(stderr, "failed to initialize EIS backend\n");
        return 1;
    }
    Devices devices;
    if (!dispatch_until(ctx, devices, 3000)) {
        std::fprintf(stderr, "no input devices became available\n");
        return 1;
    }

    for (int i = 1; i < argc;) {
        if (std::strcmp(argv[i], "key") == 0 && i + 1 < argc) {
            if (!devices.keyboard) {
                return 2;
            }
            const auto key =
                static_cast<uint32_t>(std::strtoul(argv[i + 1], nullptr, 0));
            ei_device_keyboard_key(devices.keyboard, key, true);
            frame(ctx, devices.keyboard);
            ei_device_keyboard_key(devices.keyboard, key, false);
            frame(ctx, devices.keyboard);
            i += 2;
        } else if (std::strcmp(argv[i], "hold") == 0 && i + 2 < argc) {
            if (!devices.keyboard) {
                return 2;
            }
            const auto key =
                static_cast<uint32_t>(std::strtoul(argv[i + 1], nullptr, 0));
            const int ms = std::atoi(argv[i + 2]);
            ei_device_keyboard_key(devices.keyboard, key, true);
            frame(ctx, devices.keyboard);
            std::this_thread::sleep_for(std::chrono::milliseconds(ms));
            ei_device_keyboard_key(devices.keyboard, key, false);
            frame(ctx, devices.keyboard);
            i += 3;
        } else if (std::strcmp(argv[i], "move") == 0 && i + 2 < argc) {
            if (!devices.pointer) {
                return 2;
            }
            ei_device_pointer_motion(devices.pointer, std::atof(argv[i + 1]),
                                     std::atof(argv[i + 2]));
            frame(ctx, devices.pointer);
            i += 3;
        } else if (std::strcmp(argv[i], "scroll") == 0 && i + 1 < argc) {
            if (!devices.scroll) {
                return 2;
            }
            ei_device_scroll_discrete(devices.scroll, 0,
                                      std::atoi(argv[i + 1]));
            frame(ctx, devices.scroll);
            ei_device_scroll_stop(devices.scroll, false, true);
            frame(ctx, devices.scroll);
            i += 2;
        } else if (std::strcmp(argv[i], "wait") == 0 && i + 1 < argc) {
            std::this_thread::sleep_for(
                std::chrono::milliseconds(std::atoi(argv[i + 1])));
            i += 2;
        } else {
            std::fprintf(
                stderr,
                "usage: %s (key CODE|hold CODE MS|move DX DY|scroll UNITS|wait MS)...\n",
                argv[0]);
            return 2;
        }
    }

    if (devices.keyboard) {
        ei_device_unref(devices.keyboard);
    }
    if (devices.pointer) {
        ei_device_unref(devices.pointer);
    }
    if (devices.scroll) {
        ei_device_unref(devices.scroll);
    }
    ei_unref(ctx);
    remote.call("disconnect", cookie);
    return 0;
}
