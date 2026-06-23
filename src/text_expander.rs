#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct TextExpansionRule {
    #[serde(default = "default_rule_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub trigger: String,
    #[serde(default)]
    pub replacement: String,
}

fn default_rule_enabled() -> bool {
    true
}

impl Default for TextExpansionRule {
    fn default() -> Self {
        Self {
            enabled: true,
            trigger: String::new(),
            replacement: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextExpansionConfig {
    pub enabled: bool,
    pub rules: Vec<TextExpansionRule>,
    pub app_blacklist: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextExpansionMatch {
    pub typed_trigger_chars: usize,
    pub replacement: String,
    pub cursor_back_chars: usize,
}

#[derive(Clone, Debug)]
pub struct TextExpansionEngine {
    rules: Vec<TextExpansionRule>,
    buffer: String,
    max_buffer_chars: usize,
}

impl Default for TextExpansionEngine {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl TextExpansionEngine {
    pub fn new(rules: Vec<TextExpansionRule>) -> Self {
        let max_trigger_chars = rules
            .iter()
            .filter(|rule| rule.enabled && !rule.trigger.is_empty())
            .map(|rule| rule.trigger.chars().count())
            .max()
            .unwrap_or(0);
        Self {
            rules,
            buffer: String::new(),
            max_buffer_chars: max_trigger_chars.clamp(64, 128),
        }
    }

    pub fn set_rules(&mut self, rules: Vec<TextExpansionRule>) {
        *self = Self::new(rules);
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    pub fn backspace(&mut self) {
        self.buffer.pop();
    }

    pub fn push_char(&mut self, ch: char) -> Option<TextExpansionMatch> {
        if ch.is_control() {
            self.reset();
            return None;
        }
        self.buffer.push(ch);
        self.trim_buffer();
        self.find_match().inspect(|_| self.reset())
    }

    fn trim_buffer(&mut self) {
        let len = self.buffer.chars().count();
        if len <= self.max_buffer_chars {
            return;
        }
        let keep_from = len - self.max_buffer_chars;
        let byte_idx = self
            .buffer
            .char_indices()
            .nth(keep_from)
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        self.buffer.drain(..byte_idx);
    }

    fn find_match(&self) -> Option<TextExpansionMatch> {
        self.rules
            .iter()
            .filter(|rule| rule_usable(rule))
            .flat_map(|rule| {
                activation_triggers(&rule.trigger)
                    .into_iter()
                    .map(move |trigger| (rule, trigger))
            })
            .filter(|(_, trigger)| self.buffer.ends_with(&trigger.text))
            .filter(|(_, trigger)| self.boundary_ok(&trigger.text))
            .filter(|(_, trigger)| !self.has_longer_pending_trigger(trigger))
            .max_by_key(|(_, trigger)| (trigger.text.chars().count(), trigger.priority))
            .map(|(rule, trigger)| {
                let (replacement, cursor_back_chars) = prepare_replacement(&rule.replacement);
                TextExpansionMatch {
                    typed_trigger_chars: trigger.text.chars().count(),
                    replacement,
                    cursor_back_chars,
                }
            })
    }

    fn has_longer_pending_trigger(&self, trigger: &ActivationTrigger) -> bool {
        self.rules.iter().any(|rule| {
            rule_usable(rule)
                && activation_triggers(&rule.trigger)
                    .into_iter()
                    .any(|candidate| {
                        candidate.priority >= trigger.priority
                            && candidate.text.chars().count() > trigger.text.chars().count()
                            && candidate.text.starts_with(&trigger.text)
                    })
        })
    }

    fn boundary_ok(&self, trigger: &str) -> bool {
        let Some(first) = trigger.chars().next() else {
            return false;
        };
        if !is_word_char(first) {
            return true;
        }
        let buffer_chars = self.buffer.chars().collect::<Vec<_>>();
        let trigger_len = trigger.chars().count();
        if buffer_chars.len() <= trigger_len {
            return true;
        }
        !is_word_char(buffer_chars[buffer_chars.len() - trigger_len - 1])
    }
}

pub fn rule_usable(rule: &TextExpansionRule) -> bool {
    rule.enabled
        && runtime_trigger_usable(&rule.trigger)
        && !prepare_replacement(&rule.replacement).0.is_empty()
}

fn runtime_trigger_usable(trigger: &str) -> bool {
    valid_trigger(trigger) || valid_legacy_trigger_stem(trigger)
}

fn valid_legacy_trigger_stem(trigger: &str) -> bool {
    let trimmed = trigger.trim();
    trimmed == trigger
        && trigger.chars().count() >= 1
        && !matches!(trigger.chars().next(), Some(':') | Some(';'))
        && !trigger.chars().any(char::is_whitespace)
        && !trigger.chars().any(char::is_control)
}

#[derive(Clone, Debug)]
struct ActivationTrigger {
    text: String,
    priority: TriggerMatchPriority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TriggerMatchPriority {
    Alias,
    Primary,
}

fn activation_triggers(trigger: &str) -> Vec<ActivationTrigger> {
    let base = if valid_trigger(trigger) {
        vec![trigger.to_owned()]
    } else if valid_legacy_trigger_stem(trigger) {
        vec![format!(":{trigger}"), format!(";{trigger}")]
    } else {
        return Vec::new();
    };

    let mut triggers = Vec::with_capacity(base.len() * 2);
    for trigger in base {
        push_unique_activation_trigger(
            &mut triggers,
            trigger.clone(),
            TriggerMatchPriority::Primary,
        );
        for alias in qwerty_jcuken_aliases(&trigger) {
            push_unique_activation_trigger(&mut triggers, alias, TriggerMatchPriority::Alias);
        }
    }
    triggers
}

fn push_unique_activation_trigger(
    triggers: &mut Vec<ActivationTrigger>,
    text: String,
    priority: TriggerMatchPriority,
) {
    if let Some(existing) = triggers.iter_mut().find(|item| item.text == text) {
        existing.priority = existing.priority.max(priority);
    } else {
        triggers.push(ActivationTrigger { text, priority });
    }
}

fn push_unique_trigger(triggers: &mut Vec<String>, trigger: String) {
    if !triggers.iter().any(|item| item == &trigger) {
        triggers.push(trigger);
    }
}

fn qwerty_jcuken_aliases(trigger: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    if let Some(alias) = qwerty_jcuken_alias(trigger, false) {
        push_unique_trigger(&mut aliases, alias);
    }
    if matches!(trigger.chars().next(), Some(':') | Some(';')) {
        if let Some(alias) = qwerty_jcuken_prefix_alias(trigger) {
            push_unique_trigger(&mut aliases, alias);
        }
        if let Some(alias) = qwerty_jcuken_alias(trigger, true) {
            push_unique_trigger(&mut aliases, alias);
        }
        for alias in qwerty_jcuken_physical_prefix_aliases(trigger) {
            push_unique_trigger(&mut aliases, alias);
        }
    }
    aliases
}

fn qwerty_jcuken_physical_prefix_aliases(trigger: &str) -> Vec<String> {
    let Some(first) = trigger.chars().next() else {
        return Vec::new();
    };
    let physical_prefixes: &[char] = match first {
        ':' => &['^', '6'],
        ';' => &['$', '4'],
        _ => return Vec::new(),
    };
    let rest = trigger.chars().skip(1).collect::<String>();
    let mapped_rest = qwerty_jcuken_alias(trigger, false)
        .map(|mapped| mapped.chars().skip(1).collect::<String>());
    let mut aliases = Vec::new();
    for physical_prefix in physical_prefixes {
        push_unique_trigger(&mut aliases, format!("{physical_prefix}{rest}"));
        if let Some(mapped_rest) = &mapped_rest {
            push_unique_trigger(&mut aliases, format!("{physical_prefix}{mapped_rest}"));
        }
    }
    aliases
}

fn qwerty_jcuken_alias(trigger: &str, map_prefix: bool) -> Option<String> {
    let mut changed = false;
    let mut output = String::with_capacity(trigger.len());
    for (idx, ch) in trigger.chars().enumerate() {
        if idx == 0 && matches!(ch, ':' | ';') && !map_prefix {
            output.push(ch);
        } else if let Some(mapped) = qwerty_jcuken_char_alias(ch) {
            output.push(mapped);
            changed = true;
        } else {
            output.push(ch);
        }
    }
    changed.then_some(output)
}

fn qwerty_jcuken_prefix_alias(trigger: &str) -> Option<String> {
    let mut chars = trigger.chars();
    let first = chars.next()?;
    let mapped = qwerty_jcuken_char_alias(first)?;
    let rest = chars.collect::<String>();
    Some(format!("{mapped}{rest}"))
}

fn qwerty_jcuken_char_alias(ch: char) -> Option<char> {
    match ch {
        'q' => Some('й'),
        'w' => Some('ц'),
        'e' => Some('у'),
        'r' => Some('к'),
        't' => Some('е'),
        'y' => Some('н'),
        'u' => Some('г'),
        'i' => Some('ш'),
        'o' => Some('щ'),
        'p' => Some('з'),
        '[' => Some('х'),
        ']' => Some('ъ'),
        'a' => Some('ф'),
        's' => Some('ы'),
        'd' => Some('в'),
        'f' => Some('а'),
        'g' => Some('п'),
        'h' => Some('р'),
        'j' => Some('о'),
        'k' => Some('л'),
        'l' => Some('д'),
        ';' => Some('ж'),
        '\'' => Some('э'),
        'z' => Some('я'),
        'x' => Some('ч'),
        'c' => Some('с'),
        'v' => Some('м'),
        'b' => Some('и'),
        'n' => Some('т'),
        'm' => Some('ь'),
        ',' => Some('б'),
        '.' => Some('ю'),
        '`' => Some('ё'),
        'Q' => Some('Й'),
        'W' => Some('Ц'),
        'E' => Some('У'),
        'R' => Some('К'),
        'T' => Some('Е'),
        'Y' => Some('Н'),
        'U' => Some('Г'),
        'I' => Some('Ш'),
        'O' => Some('Щ'),
        'P' => Some('З'),
        '{' => Some('Х'),
        '}' => Some('Ъ'),
        'A' => Some('Ф'),
        'S' => Some('Ы'),
        'D' => Some('В'),
        'F' => Some('А'),
        'G' => Some('П'),
        'H' => Some('Р'),
        'J' => Some('О'),
        'K' => Some('Л'),
        'L' => Some('Д'),
        ':' => Some('Ж'),
        '"' => Some('Э'),
        'Z' => Some('Я'),
        'X' => Some('Ч'),
        'C' => Some('С'),
        'V' => Some('М'),
        'B' => Some('И'),
        'N' => Some('Т'),
        'M' => Some('Ь'),
        '<' => Some('Б'),
        '>' => Some('Ю'),
        '~' => Some('Ё'),
        'й' => Some('q'),
        'ц' => Some('w'),
        'у' => Some('e'),
        'к' => Some('r'),
        'е' => Some('t'),
        'н' => Some('y'),
        'г' => Some('u'),
        'ш' => Some('i'),
        'щ' => Some('o'),
        'з' => Some('p'),
        'х' => Some('['),
        'ъ' => Some(']'),
        'ф' => Some('a'),
        'ы' => Some('s'),
        'в' => Some('d'),
        'а' => Some('f'),
        'п' => Some('g'),
        'р' => Some('h'),
        'о' => Some('j'),
        'л' => Some('k'),
        'д' => Some('l'),
        'ж' => Some(';'),
        'э' => Some('\''),
        'я' => Some('z'),
        'ч' => Some('x'),
        'с' => Some('c'),
        'м' => Some('v'),
        'и' => Some('b'),
        'т' => Some('n'),
        'ь' => Some('m'),
        'б' => Some(','),
        'ю' => Some('.'),
        'ё' => Some('`'),
        'Й' => Some('Q'),
        'Ц' => Some('W'),
        'У' => Some('E'),
        'К' => Some('R'),
        'Е' => Some('T'),
        'Н' => Some('Y'),
        'Г' => Some('U'),
        'Ш' => Some('I'),
        'Щ' => Some('O'),
        'З' => Some('P'),
        'Х' => Some('{'),
        'Ъ' => Some('}'),
        'Ф' => Some('A'),
        'Ы' => Some('S'),
        'В' => Some('D'),
        'А' => Some('F'),
        'П' => Some('G'),
        'Р' => Some('H'),
        'О' => Some('J'),
        'Л' => Some('K'),
        'Д' => Some('L'),
        'Ж' => Some(':'),
        'Э' => Some('"'),
        'Я' => Some('Z'),
        'Ч' => Some('X'),
        'С' => Some('C'),
        'М' => Some('V'),
        'И' => Some('B'),
        'Т' => Some('N'),
        'Ь' => Some('M'),
        'Б' => Some('<'),
        'Ю' => Some('>'),
        'Ё' => Some('~'),
        _ => None,
    }
}

pub fn valid_trigger(trigger: &str) -> bool {
    let trimmed = trigger.trim();
    trimmed == trigger
        && trigger.chars().count() >= 2
        && matches!(trigger.chars().next(), Some(':') | Some(';'))
        && !trigger.chars().any(char::is_whitespace)
}

pub fn prepare_replacement(raw: &str) -> (String, usize) {
    let expanded = decode_text_escapes(raw);
    let marker = "$|$";
    if let Some(marker_byte_idx) = expanded.find(marker) {
        let before = expanded[..marker_byte_idx].to_owned();
        let after = expanded[marker_byte_idx + marker.len()..].to_owned();
        let cursor_back_chars = after.chars().count();
        (format!("{before}{after}"), cursor_back_chars)
    } else {
        (expanded, 0)
    }
}

fn decode_text_escapes(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek().copied() {
                Some('n') => {
                    chars.next();
                    output.push('\n');
                }
                Some('t') => {
                    chars.next();
                    output.push('\t');
                }
                Some('r') => {
                    chars.next();
                    output.push('\r');
                }
                Some('\\') => {
                    chars.next();
                    output.push('\\');
                }
                _ => output.push(ch),
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(trigger: &str, replacement: &str) -> TextExpansionRule {
        TextExpansionRule {
            enabled: true,
            trigger: trigger.to_owned(),
            replacement: replacement.to_owned(),
        }
    }

    #[test]
    fn expands_punctuation_trigger() {
        let mut engine = TextExpansionEngine::new(vec![rule(":addr", "Earth")]);
        let mut matched = None;
        for ch in ":addr".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(
            matched,
            Some(TextExpansionMatch {
                typed_trigger_chars: 5,
                replacement: "Earth".to_owned(),
                cursor_back_chars: 0,
            })
        );
    }

    #[test]
    fn prefers_longest_trigger() {
        let mut engine = TextExpansionEngine::new(vec![rule(":a", "A"), rule(":addr", "Earth")]);
        let mut matched = None;
        for ch in ":addr".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Earth");
    }

    #[test]
    fn legacy_trigger_stem_requires_typed_prefix() {
        let mut engine = TextExpansionEngine::new(vec![rule("addr", "Earth")]);
        for ch in " addr".chars() {
            assert!(engine.push_char(ch).is_none());
        }

        engine.reset();
        let mut matched = None;
        for ch in ":addr".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Earth");
    }

    #[test]
    fn accepts_semicolon_prefix_trigger() {
        let mut engine = TextExpansionEngine::new(vec![rule(";addr", "Earth")]);
        let mut matched = None;
        for ch in ";addr".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Earth");
    }

    #[test]
    fn expands_cyrillic_trigger() {
        let mut engine = TextExpansionEngine::new(vec![rule(":адр", "Адрес")]);
        let mut matched = None;
        for ch in ":адр".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Адрес");
    }

    #[test]
    fn expands_cyrillic_trigger_from_qwerty_alias() {
        let mut engine = TextExpansionEngine::new(vec![rule(":привет", "Здравствуйте")]);
        let mut matched = None;
        for ch in ":ghbdtn".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Здравствуйте");
    }

    #[test]
    fn expands_cyrillic_trigger_from_layout_prefix_alias() {
        let mut engine = TextExpansionEngine::new(vec![rule(":привет", "Здравствуйте")]);
        let mut matched = None;
        for ch in "Жпривет".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Здравствуйте");
    }

    #[test]
    fn expands_cyrillic_trigger_from_full_layout_alias() {
        let mut engine = TextExpansionEngine::new(vec![rule(":привет", "Здравствуйте")]);
        let mut matched = None;
        for ch in "Жghbdtn".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Здравствуйте");
    }

    #[test]
    fn expands_cyrillic_trigger_from_ru_physical_colon_alias() {
        let mut engine = TextExpansionEngine::new(vec![rule(":привет", "Здравствуйте")]);
        let mut matched = None;
        for ch in "^ghbdtn".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Здравствуйте");

        engine.reset();
        let mut matched = None;
        for ch in "^привет".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Здравствуйте");

        engine.reset();
        let mut matched = None;
        for ch in "6ghbdtn".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Здравствуйте");
    }

    #[test]
    fn expands_latin_trigger_from_jcuken_alias() {
        let mut engine = TextExpansionEngine::new(vec![rule(":ghbdtn", "Hello")]);
        let mut matched = None;
        for ch in ":привет".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Hello");
    }

    #[test]
    fn exact_trigger_beats_same_length_layout_alias() {
        let mut engine =
            TextExpansionEngine::new(vec![rule(":qq", "Latin"), rule(":йй", "Cyrillic")]);
        let mut matched = None;
        for ch in ":qq".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Latin");

        engine.reset();
        let mut matched = None;
        for ch in ":йй".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Cyrillic");
    }

    #[test]
    fn exact_trigger_beats_longer_layout_alias_prefix() {
        let mut engine =
            TextExpansionEngine::new(vec![rule(":sig", "Signature"), rule(":ышпр", "Alias")]);
        let mut matched = None;
        for ch in ":sig".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Signature");
    }

    #[test]
    fn shorter_exact_trigger_still_waits_for_longer_exact_trigger() {
        let mut engine =
            TextExpansionEngine::new(vec![rule(":sig", "Short"), rule(":siglong", "Long")]);
        let mut matched = None;
        for ch in ":siglong".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(matched.unwrap().replacement, "Long");
    }

    #[test]
    fn backspace_updates_buffer() {
        let mut engine = TextExpansionEngine::new(vec![rule(":addr", "Earth")]);
        for ch in ":adx".chars() {
            assert!(engine.push_char(ch).is_none());
        }
        engine.backspace();
        assert!(engine.push_char('d').is_none());
        assert_eq!(engine.push_char('r').unwrap().replacement, "Earth");
    }

    #[test]
    fn prepares_multiline_and_cursor_marker() {
        let mut engine = TextExpansionEngine::new(vec![rule(":sig", "Hello\n$|$World")]);
        let mut matched = None;
        for ch in ":sig".chars() {
            matched = engine.push_char(ch);
        }
        assert_eq!(
            matched.unwrap(),
            TextExpansionMatch {
                typed_trigger_chars: 4,
                replacement: "Hello\nWorld".to_owned(),
                cursor_back_chars: 5,
            }
        );
    }
}
