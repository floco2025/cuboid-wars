#include <QCoreApplication>
#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusMessage>
#include <QDBusUnixFileDescriptor>
#include <QVariant>

#include "input_actions.hpp"

#include <libei.h>
#include <poll.h>
#include <unistd.h>

#include <chrono>
#include <cstdio>
#include <thread>

namespace {

// Every emitted frame is followed by this settle so the compositor delivers
// events in order; `hold` therefore lasts its MS plus two settles.
constexpr int kSettleMs = 60;
const char *kUsage =
    "usage: %s (key CODE|hold CODE MS|move DX DY|scroll CLICKS|click left|right|wait MS)...\n";

using review_input::Action;

struct Devices {
    ei_device *keyboard = nullptr;
    ei_device *pointer = nullptr;
    ei_device *button = nullptr;
    ei_device *scroll = nullptr;
};

// Owns the compositor session end to end: every exit path, error or not,
// releases the devices and context and closes the remote-desktop session.
class Session {
  public:
    Session(QDBusInterface &remote, int cookie, ei *ctx) : remote_(remote), cookie_(cookie), ctx_(ctx) {}
    Session(const Session &) = delete;
    Session &operator=(const Session &) = delete;
    ~Session() {
        for (ei_device *device : {devices.keyboard, devices.pointer, devices.button, devices.scroll}) {
            if (device) {
                ei_device_unref(device);
            }
        }
        ei_unref(ctx_);
        remote_.call("disconnect", cookie_);
    }

    ei *ctx() const { return ctx_; }

    bool dispatch_until(int timeout_ms) {
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);
        while (std::chrono::steady_clock::now() < deadline) {
            pollfd pfd{ei_get_fd(ctx_), POLLIN, 0};
            poll(&pfd, 1, 50);
            ei_dispatch(ctx_);
            while (ei_event *event = ei_get_event(ctx_)) {
                const auto type = ei_event_get_type(event);
                if (type == EI_EVENT_SEAT_ADDED) {
                    ei_seat_bind_capabilities(ei_event_get_seat(event), EI_DEVICE_CAP_POINTER,
                                              EI_DEVICE_CAP_KEYBOARD, EI_DEVICE_CAP_BUTTON,
                                              EI_DEVICE_CAP_SCROLL, nullptr);
                } else if (type == EI_EVENT_DEVICE_RESUMED) {
                    ei_device *device = ei_event_get_device(event);
                    ei_device_start_emulating(device, 1);
                    adopt(devices.keyboard, device, EI_DEVICE_CAP_KEYBOARD);
                    adopt(devices.pointer, device, EI_DEVICE_CAP_POINTER);
                    adopt(devices.button, device, EI_DEVICE_CAP_BUTTON);
                    adopt(devices.scroll, device, EI_DEVICE_CAP_SCROLL);
                } else if (type == EI_EVENT_DISCONNECT) {
                    ei_event_unref(event);
                    return false;
                }
                ei_event_unref(event);
            }
            if (devices.keyboard && devices.pointer && devices.button && devices.scroll) {
                return true;
            }
        }
        return devices.keyboard || devices.pointer || devices.button || devices.scroll;
    }

    void frame(ei_device *device) {
        ei_device_frame(device, ei_now(ctx_));
        ei_dispatch(ctx_);
        std::this_thread::sleep_for(std::chrono::milliseconds(kSettleMs));
    }

    Devices devices;

  private:
    static void adopt(ei_device *&slot, ei_device *device, ei_device_capability capability) {
        if (!slot && ei_device_has_capability(device, capability)) {
            slot = ei_device_ref(device);
        }
    }

    QDBusInterface &remote_;
    int cookie_;
    ei *ctx_;
};

const char *device_name(Action::Kind kind) {
    switch (kind) {
    case Action::Key:
    case Action::Hold:
        return "keyboard";
    case Action::Move:
        return "pointer";
    case Action::Click:
        return "button";
    case Action::Scroll:
        return "scroll";
    case Action::Wait:
        return "";
    }
    return "";
}

ei_device *device_for(const Devices &devices, Action::Kind kind) {
    switch (kind) {
    case Action::Key:
    case Action::Hold:
        return devices.keyboard;
    case Action::Move:
        return devices.pointer;
    case Action::Click:
        return devices.button;
    case Action::Scroll:
        return devices.scroll;
    case Action::Wait:
        return nullptr;
    }
    return nullptr;
}

} // namespace

int main(int argc, char **argv) {
    const auto actions = review_input::parse_actions(argc, argv);
    if (!actions) {
        std::fprintf(stderr, kUsage, argv[0]);
        return 2;
    }

    QCoreApplication app(argc, argv);
    QDBusInterface remote("org.kde.KWin", "/org/kde/KWin/EIS/RemoteDesktop", "org.kde.KWin.EIS.RemoteDesktop",
                          QDBusConnection::sessionBus());
    QDBusMessage reply = remote.call("connectToEIS", 3);
    if (reply.type() == QDBusMessage::ErrorMessage || reply.arguments().size() < 2) {
        std::fprintf(stderr, "connectToEIS failed: %s\n", reply.errorMessage().toUtf8().constData());
        return 1;
    }
    const auto descriptor = qvariant_cast<QDBusUnixFileDescriptor>(reply.arguments().at(0));
    const int cookie = reply.arguments().at(1).toInt();
    const int fd = dup(descriptor.fileDescriptor());
    if (fd < 0) {
        std::perror("dup");
        remote.call("disconnect", cookie);
        return 1;
    }

    ei *ctx = ei_new_sender(nullptr);
    if (!ctx) {
        std::fprintf(stderr, "failed to create the EIS context\n");
        close(fd);
        remote.call("disconnect", cookie);
        return 1;
    }
    Session session(remote, cookie, ctx);
    ei_configure_name(ctx, "Cuboid Wars visual review");
    // The context owns the descriptor from here on.
    if (ei_setup_backend_fd(ctx, fd) < 0) {
        std::fprintf(stderr, "failed to initialize the EIS backend\n");
        close(fd);
        return 1;
    }
    if (!session.dispatch_until(3000)) {
        std::fprintf(stderr, "no input devices became available\n");
        return 1;
    }
    for (const Action &action : *actions) {
        if (action.kind != Action::Wait && !device_for(session.devices, action.kind)) {
            std::fprintf(stderr, "the compositor offered no %s device\n", device_name(action.kind));
            return 1;
        }
    }

    for (const Action &action : *actions) {
        ei_device *device = device_for(session.devices, action.kind);
        switch (action.kind) {
        case Action::Key:
            ei_device_keyboard_key(device, action.code, true);
            session.frame(device);
            ei_device_keyboard_key(device, action.code, false);
            session.frame(device);
            break;
        case Action::Hold:
            ei_device_keyboard_key(device, action.code, true);
            session.frame(device);
            std::this_thread::sleep_for(std::chrono::milliseconds(action.ms));
            ei_device_keyboard_key(device, action.code, false);
            session.frame(device);
            break;
        case Action::Move:
            ei_device_pointer_motion(device, action.dx, action.dy);
            session.frame(device);
            break;
        case Action::Scroll:
            ei_device_scroll_discrete(device, 0, action.scroll);
            session.frame(device);
            ei_device_scroll_stop(device, false, true);
            session.frame(device);
            break;
        case Action::Click:
            ei_device_button_button(device, action.code, true);
            session.frame(device);
            ei_device_button_button(device, action.code, false);
            session.frame(device);
            break;
        case Action::Wait:
            std::this_thread::sleep_for(std::chrono::milliseconds(action.ms));
            break;
        }
    }
    return 0;
}
