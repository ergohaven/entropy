// SPDX-License-Identifier: GPL-3.0-or-later
// Entropy Universal Symbols backend for Fcitx5.

#include <array>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>
#include "fcitx-utils/handlertable.h"
#include "fcitx-utils/key.h"
#include "fcitx-utils/keysym.h"
#include "fcitx-utils/keysymgen.h"
#include "fcitx/addonfactory.h"
#include "fcitx/addoninstance.h"
#include "fcitx/addonmanager.h"
#include "fcitx/event.h"
#include "fcitx/inputcontext.h"
#include "fcitx/instance.h"

namespace fcitx {
namespace {

constexpr uint16_t KC_F13 = 0x0068;
constexpr uint16_t MOD_CTRL = 0x0100;
constexpr uint16_t MOD_SHIFT = 0x0200;
constexpr uint16_t MOD_ALT = 0x0400;
constexpr uint16_t MOD_GUI = 0x0800;
constexpr uint16_t HOST_TEXT_START_TRIGGER = KC_F13 + 7;
constexpr std::size_t HOST_TEXT_COUNT_DIGITS = 2;
constexpr std::size_t HOST_TEXT_CODEPOINT_DIGITS = 7;

struct SmartSymbol {
    uint16_t trigger;
    const char *symbol;
};

const std::array<SmartSymbol, 74> SMART_SYMBOLS{{
    // F13..F19. Unmodified F20 is reserved for host-text frames.
    {KC_F13, "{"},
    {uint16_t(KC_F13 + 1), "}"},
    {uint16_t(KC_F13 + 2), "["},
    {uint16_t(KC_F13 + 3), "]"},
    {uint16_t(KC_F13 + 4), "("},
    {uint16_t(KC_F13 + 5), ")"},
    {uint16_t(KC_F13 + 6), "<"},

    // Shift+F13..F20
    {uint16_t(MOD_SHIFT | KC_F13), "!"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 1)), "\""},
    {uint16_t(MOD_SHIFT | (KC_F13 + 2)), "$"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 3)), "%"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 4)), "&"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 5)), "'"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 6)), "*"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 7)), "+"},

    // Ctrl+F13..F20
    {uint16_t(MOD_CTRL | KC_F13), "«"},
    {uint16_t(MOD_CTRL | (KC_F13 + 1)), "»"},
    {uint16_t(MOD_CTRL | (KC_F13 + 2)), "€"},
    {uint16_t(MOD_CTRL | (KC_F13 + 3)), "—"},
    {uint16_t(MOD_CTRL | (KC_F13 + 4)), "–"},
    {uint16_t(MOD_CTRL | (KC_F13 + 5)), "•"},
    {uint16_t(MOD_CTRL | (KC_F13 + 6)), "×"},
    {uint16_t(MOD_CTRL | (KC_F13 + 7)), "±"},

    // Alt+F13..F20
    {uint16_t(MOD_ALT | KC_F13), "."},
    {uint16_t(MOD_ALT | (KC_F13 + 1)), ","},
    {uint16_t(MOD_ALT | (KC_F13 + 2)), ";"},
    {uint16_t(MOD_ALT | (KC_F13 + 3)), ":"},
    {uint16_t(MOD_ALT | (KC_F13 + 4)), "/"},
    {uint16_t(MOD_ALT | (KC_F13 + 5)), "`"},
    {uint16_t(MOD_ALT | (KC_F13 + 6)), "^"},
    {uint16_t(MOD_ALT | (KC_F13 + 7)), "≠"},

    // Alt+Shift+F13..F20
    {uint16_t(MOD_ALT | MOD_SHIFT | KC_F13), "#"},
    {uint16_t(MOD_ALT | MOD_SHIFT | (KC_F13 + 1)), "@"},
    {uint16_t(MOD_ALT | MOD_SHIFT | (KC_F13 + 2)), "№"},
    {uint16_t(MOD_ALT | MOD_SHIFT | (KC_F13 + 3)), "₽"},
    {uint16_t(MOD_ALT | MOD_SHIFT | (KC_F13 + 4)), "="},
    {uint16_t(MOD_ALT | MOD_SHIFT | (KC_F13 + 5)), "?"},
    {uint16_t(MOD_ALT | MOD_SHIFT | (KC_F13 + 6)), "|"},
    {uint16_t(MOD_ALT | MOD_SHIFT | (KC_F13 + 7)), "\\"},

    // Ctrl+Alt+F13..F20
    {uint16_t(MOD_CTRL | MOD_ALT | KC_F13), "б"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 1)), "ю"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 2)), "ж"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 3)), "э"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 4)), "х"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 5)), "ъ"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 6)), "ё"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 7)), "≈"},

    // Ctrl+Alt+Shift+F13..F20
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | KC_F13), "Б"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 1)), "Ю"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 2)), "Ж"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 3)), "Э"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 4)), "Х"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 5)), "Ъ"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 6)), "Ё"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 7)), "✓"},

    // Ctrl+Shift+F13..F20
    {uint16_t(MOD_CTRL | MOD_SHIFT | KC_F13), "°"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 1)), "‰"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 2)), "′"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 3)), "″"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 4)), "‘"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 5)), "’"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 6)), "„"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 7)), "“"},

    // Super+F13..F18
    {uint16_t(MOD_GUI | KC_F13), "§"},
    {uint16_t(MOD_GUI | (KC_F13 + 1)), "”"},
    {uint16_t(MOD_GUI | (KC_F13 + 2)), "™"},
    {uint16_t(MOD_GUI | (KC_F13 + 3)), "~"},
    {uint16_t(MOD_GUI | (KC_F13 + 4)), "_"},
    {uint16_t(MOD_GUI | (KC_F13 + 5)), "-"},

    // Super+Shift+F13..F17
    {uint16_t(MOD_GUI | MOD_SHIFT | KC_F13), "←"},
    {uint16_t(MOD_GUI | MOD_SHIFT | (KC_F13 + 1)), "↑"},
    {uint16_t(MOD_GUI | MOD_SHIFT | (KC_F13 + 2)), "→"},
    {uint16_t(MOD_GUI | MOD_SHIFT | (KC_F13 + 3)), "↓"},
    {uint16_t(MOD_GUI | MOD_SHIFT | (KC_F13 + 4)), "↔"},
}};

std::optional<uint16_t> baseKeycodeForSym(KeySym sym) {
    if (sym >= FcitxKey_F13 && sym <= FcitxKey_F20) {
        return uint16_t(KC_F13 + (sym - FcitxKey_F13));
    }
    return std::nullopt;
}

uint16_t transportModifiers(KeyStates states) {
    uint16_t modifiers = 0;
    if (states.test(KeyState::Ctrl)) {
        modifiers |= MOD_CTRL;
    }
    if (states.test(KeyState::Shift)) {
        modifiers |= MOD_SHIFT;
    }
    if (states.test(KeyState::Alt)) {
        modifiers |= MOD_ALT;
    }
    if (states.test(KeyState::Super)) {
        modifiers |= MOD_GUI;
    }
    return modifiers;
}

std::optional<std::string> symbolForKey(const Key &key) {
    const auto base = baseKeycodeForSym(key.sym());
    if (!base) {
        return std::nullopt;
    }
    const uint16_t trigger = *base | transportModifiers(key.states());
    for (const auto &entry : SMART_SYMBOLS) {
        if (entry.trigger == trigger) {
            return std::string(entry.symbol);
        }
    }
    return std::nullopt;
}

struct HostTextTransportResult {
    bool handled;
    std::optional<std::string> text;
};

class HostTextTransportDecoder {
public:
    HostTextTransportResult process(uint16_t baseKeycode, uint16_t modifiers,
                                    bool released) {
        const auto now = std::chrono::steady_clock::now();
        if (active_ && now - lastEvent_ > std::chrono::seconds(2)) {
            reset();
        }

        if (!active_) {
            if (baseKeycode != HOST_TEXT_START_TRIGGER || modifiers != 0 || released) {
                return {false, std::nullopt};
            }
            active_ = true;
            lastEvent_ = now;
            return {true, std::nullopt};
        }

        lastEvent_ = now;
        if (released) {
            return {true, std::nullopt};
        }
        if (baseKeycode < KC_F13 || baseKeycode > KC_F13 + 7) {
            reset();
            return {true, std::nullopt};
        }

        const uint32_t digit = baseKeycode - KC_F13;
        if (countDigits_ < HOST_TEXT_COUNT_DIGITS) {
            codepointCount_ = (codepointCount_ << 3) | digit;
            countDigits_++;
            if (countDigits_ == HOST_TEXT_COUNT_DIGITS) {
                if (codepointCount_ == 0 || codepointCount_ > 077) {
                    reset();
                } else {
                    remaining_ = codepointCount_;
                }
            }
            return {true, std::nullopt};
        }

        codepoint_ = (codepoint_ << 3) | digit;
        codepointDigits_++;
        if (codepointDigits_ != HOST_TEXT_CODEPOINT_DIGITS) {
            return {true, std::nullopt};
        }
        if (!appendUtf8(output_, codepoint_)) {
            reset();
            return {true, std::nullopt};
        }

        remaining_--;
        codepoint_ = 0;
        codepointDigits_ = 0;
        if (remaining_ != 0) {
            return {true, std::nullopt};
        }

        auto output = std::move(output_);
        reset();
        return {true, std::move(output)};
    }

#ifdef ENTROPY_HOST_TEXT_TEST
    void expireForTest() {
        lastEvent_ = std::chrono::steady_clock::now() - std::chrono::seconds(3);
    }
#endif

private:
    static bool appendUtf8(std::string &output, uint32_t codepoint) {
        if (codepoint > 0x10FFFF ||
            (codepoint >= 0xD800 && codepoint <= 0xDFFF)) {
            return false;
        }
        if (codepoint <= 0x7F) {
            output.push_back(static_cast<char>(codepoint));
        } else if (codepoint <= 0x7FF) {
            output.push_back(static_cast<char>(0xC0 | (codepoint >> 6)));
            output.push_back(static_cast<char>(0x80 | (codepoint & 0x3F)));
        } else if (codepoint <= 0xFFFF) {
            output.push_back(static_cast<char>(0xE0 | (codepoint >> 12)));
            output.push_back(
                static_cast<char>(0x80 | ((codepoint >> 6) & 0x3F)));
            output.push_back(static_cast<char>(0x80 | (codepoint & 0x3F)));
        } else {
            output.push_back(static_cast<char>(0xF0 | (codepoint >> 18)));
            output.push_back(
                static_cast<char>(0x80 | ((codepoint >> 12) & 0x3F)));
            output.push_back(
                static_cast<char>(0x80 | ((codepoint >> 6) & 0x3F)));
            output.push_back(static_cast<char>(0x80 | (codepoint & 0x3F)));
        }
        return true;
    }

    void reset() {
        active_ = false;
        countDigits_ = 0;
        codepointCount_ = 0;
        remaining_ = 0;
        codepointDigits_ = 0;
        codepoint_ = 0;
        output_.clear();
    }

    bool active_ = false;
    std::size_t countDigits_ = 0;
    uint32_t codepointCount_ = 0;
    uint32_t remaining_ = 0;
    std::size_t codepointDigits_ = 0;
    uint32_t codepoint_ = 0;
    std::string output_;
    std::chrono::steady_clock::time_point lastEvent_{};
};

} // namespace

#ifdef ENTROPY_HOST_TEXT_TEST
int entropyHostTextTransportSelfTest() {
    const auto digits = [](uint32_t value, std::size_t count) {
        std::vector<uint16_t> output;
        output.reserve(count);
        for (std::size_t shift = count; shift-- > 0;) {
            output.push_back(KC_F13 + ((value >> (shift * 3)) & 7));
        }
        return output;
    };

    HostTextTransportDecoder decoder;
    if (!decoder.process(HOST_TEXT_START_TRIGGER, 0, false).handled) {
        return 1;
    }
    std::optional<std::string> completed;
    for (const auto keycode : digits(1, HOST_TEXT_COUNT_DIGITS)) {
        completed = decoder.process(keycode, 0, false).text;
    }
    for (const auto keycode : digits(0x1F600, HOST_TEXT_CODEPOINT_DIGITS)) {
        const auto result = decoder.process(keycode, 0, false);
        if (result.text) {
            completed = result.text;
        }
    }
    if (completed != std::optional<std::string>{"\xF0\x9F\x98\x80"}) {
        return 2;
    }

    HostTextTransportDecoder invalid;
    if (invalid.process(HOST_TEXT_START_TRIGGER, MOD_GUI, false).handled ||
        !invalid.process(HOST_TEXT_START_TRIGGER, 0, false).handled ||
        !invalid.process(KC_F13, 0, false).handled ||
        !invalid.process(KC_F13, 0, false).handled ||
        invalid.process(KC_F13, 0, false).handled) {
        return 3;
    }

    HostTextTransportDecoder interrupted;
    interrupted.process(HOST_TEXT_START_TRIGGER, 0, false);
    if (!interrupted.process(KC_F13 - 1, 0, false).handled ||
        interrupted.process(KC_F13, 0, false).handled) {
        return 4;
    }

    HostTextTransportDecoder timed_out;
    timed_out.process(HOST_TEXT_START_TRIGGER, 0, false);
    timed_out.expireForTest();
    return timed_out.process(KC_F13, 0, false).handled ? 5 : 0;
}
#endif

class EntropyUniversalSymbols final : public AddonInstance {
public:
    explicit EntropyUniversalSymbols(Instance *instance) : instance_(instance) {
        eventHandler_ = instance_->watchEvent(
            EventType::InputContextKeyEvent, EventWatcherPhase::Default,
            [this](Event &event) { handleKeyEvent(event); });
    }

private:
    void handleKeyEvent(Event &event) {
        auto &keyEvent = static_cast<KeyEvent &>(event);
        if (const auto base = baseKeycodeForSym(keyEvent.key().sym())) {
            const auto result = hostTextTransport_.process(
                *base, transportModifiers(keyEvent.key().states()),
                keyEvent.isRelease());
            if (result.handled) {
                if (result.text) {
                    keyEvent.inputContext()->commitString(*result.text);
                }
                keyEvent.filterAndAccept();
                return;
            }
        }

        const auto symbol = symbolForKey(keyEvent.key());
        if (!symbol) {
            return;
        }

        // Swallow both press and release for handled transport chords, but
        // commit text only on press.
        if (!keyEvent.isRelease()) {
            keyEvent.inputContext()->commitString(*symbol);
        }
        keyEvent.filterAndAccept();
    }

    Instance *instance_;
    HostTextTransportDecoder hostTextTransport_;
    std::unique_ptr<HandlerTableEntry<EventHandler>> eventHandler_;
};

class EntropyUniversalSymbolsFactory final : public AddonFactory {
    AddonInstance *create(AddonManager *manager) override {
        return new EntropyUniversalSymbols(manager->instance());
    }
};

} // namespace fcitx

#ifndef ENTROPY_HOST_TEXT_TEST
FCITX_ADDON_FACTORY_V2(entropyuniversalsymbols,
                       fcitx::EntropyUniversalSymbolsFactory);
#else
int main() {
    return fcitx::entropyHostTextTransportSelfTest();
}
#endif
