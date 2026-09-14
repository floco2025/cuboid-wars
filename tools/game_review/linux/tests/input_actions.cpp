#include "../input_actions.hpp"

#include <cassert>
#include <initializer_list>
#include <string>

using review_input::Action;

std::optional<std::vector<Action>> parse(std::initializer_list<const char *> words) {
    std::vector<const char *> args{"input"};
    args.insert(args.end(), words);
    return review_input::parse_actions(static_cast<int>(args.size()), args.data());
}

int main() {
    auto sequence = parse({"key", "0x2f", "hold", "17", "1500", "move", "-12.5", "0.25",
                           "scroll", "-2", "click", "left", "click", "right", "wait", "0"});
    assert(sequence && sequence->size() == 7);
    assert((*sequence)[0].kind == Action::Key && (*sequence)[0].code == 47);
    assert((*sequence)[1].kind == Action::Hold && (*sequence)[1].ms == 1500);
    assert((*sequence)[2].dx == -12.5 && (*sequence)[2].dy == 0.25);
    assert((*sequence)[3].scroll == -240);
    assert((*sequence)[4].code == BTN_LEFT && (*sequence)[5].code == BTN_RIGHT);
    assert((*sequence)[6].ms == 0);

    for (const char *invalid : {"-1", "0", "9999999999999999999999999999999999", "768", "17x", ""}) {
        assert(!parse({"key", invalid}));
        assert(!parse({"hold", invalid, "1"}));
    }
    for (const char *invalid : {"-1", "2147483648", "9999999999999999999999999999999999"}) {
        assert(!parse({"hold", "17", invalid}));
        assert(!parse({"wait", invalid}));
    }
    for (const char *invalid : {"nan", "NaN", "inf", "-infinity", "1e999", "1e-999", "1x", ""}) {
        assert(!parse({"move", invalid, "0"}));
        assert(!parse({"move", "0", invalid}));
    }
    const int low = std::numeric_limits<int>::min() / review_input::kScrollClick;
    const int high = std::numeric_limits<int>::max() / review_input::kScrollClick;
    for (int count : {low, 0, high}) {
        auto value = std::to_string(count);
        auto scroll = parse({"scroll", value.c_str()});
        assert(scroll && (*scroll)[0].scroll == count * review_input::kScrollClick);
    }
    for (int count : {low - 1, high + 1}) {
        auto value = std::to_string(count);
        assert(!parse({"scroll", value.c_str()}));
    }
    assert(parse({"wait", "2147483647"}));
    assert(!parse({}));
    assert(!parse({"hold", "17"}));
    assert(!parse({"move", "1"}));
    assert(!parse({"click", "middle"}));
    assert(!parse({"key", "17", "wait", "20", "move", "1", "nan"}));
}
