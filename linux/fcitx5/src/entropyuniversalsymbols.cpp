// SPDX-License-Identifier: GPL-3.0-or-later
// Entropy Universal Symbols backend for Fcitx5.

#include <array>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
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
constexpr uint16_t HOST_TEXT_START_TRIGGER =
    MOD_CTRL | MOD_SHIFT | MOD_ALT | MOD_GUI | (KC_F13 + 7);
constexpr uint16_t HOST_TEXT_END_TRIGGER =
    MOD_CTRL | MOD_SHIFT | MOD_ALT | MOD_GUI | (KC_F13 + 6);
constexpr uint16_t HOST_TEXT_DATA_MODIFIERS = MOD_CTRL | MOD_ALT | MOD_GUI;
constexpr size_t HOST_TEXT_MAX_DIGITS = 3 * 1024;

struct SmartSymbol {
    uint16_t trigger;
    const char *symbol;
};

const std::array<SmartSymbol, 75> SMART_SYMBOLS{{
    // F13..F20
    {KC_F13, "{"},
    {uint16_t(KC_F13 + 1), "}"},
    {uint16_t(KC_F13 + 2), "["},
    {uint16_t(KC_F13 + 3), "]"},
    {uint16_t(KC_F13 + 4), "("},
    {uint16_t(KC_F13 + 5), ")"},
    {uint16_t(KC_F13 + 6), "<"},
    {uint16_t(KC_F13 + 7), ">"},

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

bool isHostTextTransportTrigger(uint16_t trigger) {
    const auto base = trigger & 0x00ff;
    const auto modifiers = trigger & 0xff00;
    return trigger == HOST_TEXT_START_TRIGGER || trigger == HOST_TEXT_END_TRIGGER ||
           (modifiers == HOST_TEXT_DATA_MODIFIERS && base >= KC_F13 && base <= KC_F13 + 7);
}

bool isTransportModifierKey(KeySym sym) {
    switch (sym) {
    case FcitxKey_Shift_L:
    case FcitxKey_Shift_R:
    case FcitxKey_Control_L:
    case FcitxKey_Control_R:
    case FcitxKey_Alt_L:
    case FcitxKey_Alt_R:
    case FcitxKey_Super_L:
    case FcitxKey_Super_R:
        return true;
    default:
        return false;
    }
}

bool isValidUtf8(const std::string &text) {
    for (size_t index = 0; index < text.size();) {
        const auto byte = static_cast<uint8_t>(text[index]);
        size_t width = 0;
        if (byte <= 0x7f) {
            width = 1;
        } else if (byte >= 0xc2 && byte <= 0xdf) {
            width = 2;
        } else if (byte >= 0xe0 && byte <= 0xef) {
            width = 3;
        } else if (byte >= 0xf0 && byte <= 0xf4) {
            width = 4;
        } else {
            return false;
        }
        if (index + width > text.size()) {
            return false;
        }
        for (size_t continuation = 1; continuation < width; ++continuation) {
            if ((static_cast<uint8_t>(text[index + continuation]) & 0xc0) != 0x80) {
                return false;
            }
        }
        if ((byte == 0xe0 && static_cast<uint8_t>(text[index + 1]) < 0xa0) ||
            (byte == 0xed && static_cast<uint8_t>(text[index + 1]) >= 0xa0) ||
            (byte == 0xf0 && static_cast<uint8_t>(text[index + 1]) < 0x90) ||
            (byte == 0xf4 && static_cast<uint8_t>(text[index + 1]) >= 0x90)) {
            return false;
        }
        index += width;
    }
    return true;
}

std::optional<std::string> decodeHostTextDigits(const std::vector<uint8_t> &digits) {
    if (digits.size() % 3 != 0) {
        return std::nullopt;
    }
    std::string text;
    text.reserve(digits.size() / 3);
    for (size_t index = 0; index < digits.size(); index += 3) {
        if (digits[index] > 3 || digits[index + 1] > 7 || digits[index + 2] > 7) {
            return std::nullopt;
        }
        text.push_back(char((digits[index] << 6) | (digits[index + 1] << 3) | digits[index + 2]));
    }
    return isValidUtf8(text) ? std::optional<std::string>(text) : std::nullopt;
}

} // namespace

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
        const auto base = baseKeycodeForSym(keyEvent.key().sym());
        if (base) {
            const auto trigger = uint16_t(*base | transportModifiers(keyEvent.key().states()));
            if (isHostTextTransportTrigger(trigger)) {
                if (!keyEvent.isRelease()) {
                    if (trigger == HOST_TEXT_START_TRIGGER) {
                        hostTextActive_ = true;
                        hostTextDigits_.clear();
                    } else if (trigger == HOST_TEXT_END_TRIGGER && hostTextActive_) {
                        hostTextActive_ = false;
                        if (const auto text = decodeHostTextDigits(hostTextDigits_)) {
                            keyEvent.inputContext()->commitString(*text);
                        }
                        hostTextDigits_.clear();
                    } else if (hostTextActive_ &&
                               (trigger & 0xff00) == HOST_TEXT_DATA_MODIFIERS) {
                        if (hostTextDigits_.size() < HOST_TEXT_MAX_DIGITS) {
                            hostTextDigits_.push_back(uint8_t((trigger & 0x00ff) - KC_F13));
                        } else {
                            hostTextActive_ = false;
                            hostTextDigits_.clear();
                        }
                    }
                }
                keyEvent.filterAndAccept();
                return;
            }
        }
        if (!keyEvent.isRelease() && !isTransportModifierKey(keyEvent.key().sym())) {
            hostTextActive_ = false;
            hostTextDigits_.clear();
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
    std::unique_ptr<HandlerTableEntry<EventHandler>> eventHandler_;
    bool hostTextActive_ = false;
    std::vector<uint8_t> hostTextDigits_;
};

class EntropyUniversalSymbolsFactory final : public AddonFactory {
    AddonInstance *create(AddonManager *manager) override {
        return new EntropyUniversalSymbols(manager->instance());
    }
};

} // namespace fcitx

FCITX_ADDON_FACTORY_V2(entropyuniversalsymbols,
                       fcitx::EntropyUniversalSymbolsFactory);
