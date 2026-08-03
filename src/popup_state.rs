use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PopupKey {
    PickerWindow,
    MacroKeyPickWindow,
    RegularKeyPickWindow,
    PickLayerWindow,
    PendingKeyPickWindow,
    TdKeyPickWindow,
}

#[derive(Debug, Default, Clone)]
pub struct PopupState {
    epochs: HashMap<PopupKey, u64>,
    open: HashSet<PopupKey>,
}

impl PopupState {
    pub fn on_open(&mut self, key: PopupKey) {
        if self.open.insert(key) {
            *self.epochs.entry(key).or_insert(0) += 1;
        }
    }

    pub fn on_close(&mut self, key: PopupKey) {
        self.open.remove(&key);
    }

    pub fn begin_frame(&mut self, key: PopupKey, is_open: bool) {
        if is_open {
            self.on_open(key);
        } else {
            self.on_close(key);
        }
    }

    pub fn id(&self, key: PopupKey) -> egui::Id {
        egui::Id::new(("popup", key, self.epochs.get(&key).copied().unwrap_or(0)))
    }
}
