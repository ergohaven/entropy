// SPDX-License-Identifier: GPL-3.0-or-later
// Entropy Universal Symbols backend for Fcitx5.

#include <array>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
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

struct SmartSymbol {
    uint16_t trigger;
    const char *symbol;
};

const std::array<SmartSymbol, 79> SMART_SYMBOLS{{
    // F13..F24
    {KC_F13, "{"},
    {uint16_t(KC_F13 + 1), "}"},
    {uint16_t(KC_F13 + 2), "["},
    {uint16_t(KC_F13 + 3), "]"},
    {uint16_t(KC_F13 + 4), "("},
    {uint16_t(KC_F13 + 5), ")"},
    {uint16_t(KC_F13 + 6), "<"},
    {uint16_t(KC_F13 + 7), ">"},
    {uint16_t(KC_F13 + 8), "#"},
    {uint16_t(KC_F13 + 9), "@"},
    {uint16_t(KC_F13 + 10), "№"},
    {uint16_t(KC_F13 + 11), "₽"},

    // Shift+F13..F24
    {uint16_t(MOD_SHIFT | KC_F13), "!"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 1)), "\""},
    {uint16_t(MOD_SHIFT | (KC_F13 + 2)), "$"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 3)), "%"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 4)), "&"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 5)), "'"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 6)), "*"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 7)), "+"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 8)), "="},
    {uint16_t(MOD_SHIFT | (KC_F13 + 9)), "?"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 10)), "|"},
    {uint16_t(MOD_SHIFT | (KC_F13 + 11)), "\\"},

    // Ctrl+F13..F24
    {uint16_t(MOD_CTRL | KC_F13), "«"},
    {uint16_t(MOD_CTRL | (KC_F13 + 1)), "»"},
    {uint16_t(MOD_CTRL | (KC_F13 + 2)), "€"},
    {uint16_t(MOD_CTRL | (KC_F13 + 3)), "—"},
    {uint16_t(MOD_CTRL | (KC_F13 + 4)), "–"},
    {uint16_t(MOD_CTRL | (KC_F13 + 5)), "•"},
    {uint16_t(MOD_CTRL | (KC_F13 + 6)), "×"},
    {uint16_t(MOD_CTRL | (KC_F13 + 7)), "±"},
    {uint16_t(MOD_CTRL | (KC_F13 + 8)), "≠"},
    {uint16_t(MOD_CTRL | (KC_F13 + 9)), "≈"},
    {uint16_t(MOD_CTRL | (KC_F13 + 10)), "✓"},
    {uint16_t(MOD_CTRL | (KC_F13 + 11)), "§"},

    // Alt+F13..F24
    {uint16_t(MOD_ALT | KC_F13), "."},
    {uint16_t(MOD_ALT | (KC_F13 + 1)), ","},
    {uint16_t(MOD_ALT | (KC_F13 + 2)), ";"},
    {uint16_t(MOD_ALT | (KC_F13 + 3)), ":"},
    {uint16_t(MOD_ALT | (KC_F13 + 4)), "/"},
    {uint16_t(MOD_ALT | (KC_F13 + 5)), "`"},
    {uint16_t(MOD_ALT | (KC_F13 + 6)), "^"},
    {uint16_t(MOD_ALT | (KC_F13 + 7)), "←"},
    {uint16_t(MOD_ALT | (KC_F13 + 8)), "↑"},
    {uint16_t(MOD_ALT | (KC_F13 + 9)), "→"},
    {uint16_t(MOD_ALT | (KC_F13 + 10)), "↓"},
    {uint16_t(MOD_ALT | (KC_F13 + 11)), "↔"},

    // Ctrl+Alt+F13..F19
    {uint16_t(MOD_CTRL | MOD_ALT | KC_F13), "б"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 1)), "ю"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 2)), "ж"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 3)), "э"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 4)), "х"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 5)), "ъ"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 6)), "ё"},

    // Ctrl+Alt+F20..F24
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 7)), "🙂"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 8)), "😀"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 9)), "😂"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 10)), "😉"},
    {uint16_t(MOD_CTRL | MOD_ALT | (KC_F13 + 11)), "😅"},

    // Ctrl+Alt+Shift+F13..F19
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | KC_F13), "Б"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 1)), "Ю"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 2)), "Ж"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 3)), "Э"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 4)), "Х"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 5)), "Ъ"},
    {uint16_t(MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 6)), "Ё"},

    // Ctrl+Shift+F13..F24
    {uint16_t(MOD_CTRL | MOD_SHIFT | KC_F13), "°"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 1)), "‰"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 2)), "′"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 3)), "″"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 4)), "‘"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 5)), "’"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 6)), "„"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 7)), "“"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 8)), "”"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 9)), "™"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 10)), "~"},
    {uint16_t(MOD_CTRL | MOD_SHIFT | (KC_F13 + 11)), "_"},
}};

std::optional<uint16_t> baseKeycodeForSym(KeySym sym) {
    if (sym >= FcitxKey_F13 && sym <= FcitxKey_F24) {
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
};

class EntropyUniversalSymbolsFactory final : public AddonFactory {
    AddonInstance *create(AddonManager *manager) override {
        return new EntropyUniversalSymbols(manager->instance());
    }
};

} // namespace fcitx

FCITX_ADDON_FACTORY_V2(entropyuniversalsymbols,
                       fcitx::EntropyUniversalSymbolsFactory);
