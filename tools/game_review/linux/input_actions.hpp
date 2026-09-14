#pragma once

#include <linux/input-event-codes.h>

#include <cerrno>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <optional>
#include <vector>

namespace review_input {

// libei counts discrete scrolling in 120ths of a wheel click.
constexpr int kScrollClick = 120;

struct Action {
    enum Kind { Key, Hold, Move, Scroll, Click, Wait } kind;
    uint32_t code = 0;
    int ms = 0;
    double dx = 0.0;
    double dy = 0.0;
    int scroll = 0;
};

inline bool parse_integer(const char *text, long minimum, long maximum, long &value) {
    char *end = nullptr;
    errno = 0;
    value = std::strtol(text, &end, 0);
    return *text != '\0' && *end == '\0' && errno != ERANGE && value >= minimum && value <= maximum;
}

inline bool parse_real(const char *text, double &value) {
    char *end = nullptr;
    errno = 0;
    value = std::strtod(text, &end);
    return *text != '\0' && *end == '\0' && errno != ERANGE && std::isfinite(value);
}

// A partial sequence never reaches the desktop session.
inline std::optional<std::vector<Action>> parse_actions(int argc, const char *const *argv) {
    std::vector<Action> actions;
    for (int i = 1; i < argc;) {
        const char *word = argv[i];
        long integer = 0;
        double real = 0.0;
        if (std::strcmp(word, "key") == 0 && i + 1 < argc &&
            parse_integer(argv[i + 1], KEY_RESERVED + 1, KEY_MAX, integer)) {
            actions.push_back({.kind = Action::Key, .code = static_cast<uint32_t>(integer)});
            i += 2;
        } else if (std::strcmp(word, "hold") == 0 && i + 2 < argc &&
                   parse_integer(argv[i + 1], KEY_RESERVED + 1, KEY_MAX, integer)) {
            long ms = 0;
            if (!parse_integer(argv[i + 2], 0, std::numeric_limits<int>::max(), ms)) {
                return std::nullopt;
            }
            actions.push_back({.kind = Action::Hold, .code = static_cast<uint32_t>(integer), .ms = static_cast<int>(ms)});
            i += 3;
        } else if (std::strcmp(word, "move") == 0 && i + 2 < argc && parse_real(argv[i + 1], real)) {
            double dy = 0.0;
            if (!parse_real(argv[i + 2], dy)) {
                return std::nullopt;
            }
            actions.push_back({.kind = Action::Move, .dx = real, .dy = dy});
            i += 3;
        } else if (std::strcmp(word, "scroll") == 0 && i + 1 < argc &&
                   parse_integer(argv[i + 1], std::numeric_limits<int>::min() / kScrollClick,
                                 std::numeric_limits<int>::max() / kScrollClick, integer)) {
            actions.push_back({.kind = Action::Scroll, .scroll = static_cast<int>(integer) * kScrollClick});
            i += 2;
        } else if (std::strcmp(word, "click") == 0 && i + 1 < argc) {
            if (std::strcmp(argv[i + 1], "left") == 0) {
                actions.push_back({.kind = Action::Click, .code = BTN_LEFT});
            } else if (std::strcmp(argv[i + 1], "right") == 0) {
                actions.push_back({.kind = Action::Click, .code = BTN_RIGHT});
            } else {
                return std::nullopt;
            }
            i += 2;
        } else if (std::strcmp(word, "wait") == 0 && i + 1 < argc &&
                   parse_integer(argv[i + 1], 0, std::numeric_limits<int>::max(), integer)) {
            actions.push_back({.kind = Action::Wait, .ms = static_cast<int>(integer)});
            i += 2;
        } else {
            return std::nullopt;
        }
    }
    if (actions.empty()) {
        return std::nullopt;
    }
    return actions;
}

} // namespace review_input
