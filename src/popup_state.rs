use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PopupWindow {
    Picker,
    MacroKeyPick,
    RegularKeyPick,
    PickLayer,
    PendingKeyPick,
    TdKeyPick,
}

#[derive(Debug, Default, Clone)]
pub struct PopupState {
    epochs: HashMap<PopupWindow, u64>,
    open: HashSet<PopupWindow>,
}

impl PopupState {
    pub fn on_open(&mut self, key: PopupWindow) {
        if self.open.insert(key) {
            *self.epochs.entry(key).or_insert(0) += 1;
        }
    }

    pub fn on_close(&mut self, key: PopupWindow) {
        self.open.remove(&key);
    }

    pub fn begin_frame(&mut self, key: PopupWindow, is_open: bool) {
        if is_open {
            self.on_open(key);
        } else {
            self.on_close(key);
        }
    }

    pub fn id(&self, key: PopupWindow) -> egui::Id {
        egui::Id::new(("popup", key, self.epochs.get(&key).copied().unwrap_or(0)))
    }
}
