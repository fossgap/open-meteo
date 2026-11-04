//! Preserve hidden windows across a screen lock, but recover from real tray loss.
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

pub const RECONNECT_GRACE: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct TrayRecovery {
    missing_since: Option<Instant>,
}

impl TrayRecovery {
    pub fn should_restore(&mut self, reachable: bool, locked: bool, now: Instant) -> bool {
        if reachable || locked {
            self.missing_since = None;
            return false;
        }
        now.duration_since(*self.missing_since.get_or_insert(now)) >= RECONNECT_GRACE
    }
}

#[derive(Default)]
struct LockState {
    active: Cell<bool>,
    revision: Cell<u64>,
}

/// Observe existing lock services without starting a screensaver or blocking GTK.
pub struct SessionLock {
    states: Vec<Rc<LockState>>,
    _proxies: Rc<RefCell<Vec<gio::DBusProxy>>>,
}

impl SessionLock {
    pub fn new() -> Self {
        let proxies = Rc::new(RefCell::new(Vec::new()));
        let mut states = Vec::new();
        for (name, path, interface) in [
            (
                "org.gnome.Shell.ScreenShield",
                "/org/gnome/ScreenSaver",
                "org.gnome.ScreenSaver",
            ),
            (
                "org.gnome.ScreenSaver",
                "/org/gnome/ScreenSaver",
                "org.gnome.ScreenSaver",
            ),
            (
                "org.freedesktop.ScreenSaver",
                "/org/freedesktop/ScreenSaver",
                "org.freedesktop.ScreenSaver",
            ),
        ] {
            let state = Rc::new(LockState::default());
            states.push(state.clone());
            let proxies = Rc::downgrade(&proxies);
            gio::DBusProxy::for_bus(
                gio::BusType::Session,
                gio::DBusProxyFlags::DO_NOT_AUTO_START
                    | gio::DBusProxyFlags::DO_NOT_LOAD_PROPERTIES,
                None,
                name,
                path,
                interface,
                gio::Cancellable::NONE,
                move |result| {
                    let (Ok(proxy), Some(proxies)) = (result, proxies.upgrade()) else {
                        return;
                    };
                    let signal_state = state.clone();
                    proxy.connect_local("g-signal", false, move |values| {
                        if values[2].get::<String>().ok().as_deref() == Some("ActiveChanged") {
                            if let Some((active,)) = values[3]
                                .get::<glib::Variant>()
                                .ok()
                                .and_then(|parameters| parameters.get::<(bool,)>())
                            {
                                signal_state.revision.set(signal_state.revision.get() + 1);
                                signal_state.active.set(active);
                            }
                        }
                        None
                    });
                    let owner_state = state.clone();
                    proxy.connect_notify_local(Some("g-name-owner"), move |proxy, _| {
                        Self::query(proxy, &owner_state);
                    });
                    Self::query(&proxy, &state);
                    proxies.borrow_mut().push(proxy);
                },
            );
        }
        Self {
            states,
            _proxies: proxies,
        }
    }

    pub fn is_locked(&self) -> bool {
        self.states.iter().any(|state| state.active.get())
    }

    fn query(proxy: &gio::DBusProxy, state: &Rc<LockState>) {
        let revision = state.revision.get() + 1;
        state.revision.set(revision);
        if proxy.g_name_owner().is_none() {
            state.active.set(false);
            return;
        }
        let state = state.clone();
        proxy.call(
            "GetActive",
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            1000,
            gio::Cancellable::NONE,
            move |result| {
                // A signal or service replacement can overtake this reply.
                if state.revision.get() == revision {
                    state.active.set(
                        result
                            .ok()
                            .and_then(|reply| reply.get::<(bool,)>())
                            .is_some_and(|(active,)| active),
                    );
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_tray_loss_restores_after_a_bounded_grace() {
        let now = Instant::now();
        let mut recovery = TrayRecovery::default();
        assert!(!recovery.should_restore(false, false, now));
        assert!(!recovery.should_restore(false, false, now + RECONNECT_GRACE / 2));
        assert!(recovery.should_restore(false, false, now + RECONNECT_GRACE));
    }

    #[test]
    fn a_long_lock_preserves_hidden_windows_and_starts_a_fresh_grace_on_unlock() {
        let now = Instant::now();
        let mut recovery = TrayRecovery::default();
        // The tray may disappear before the lock signal arrives.
        assert!(!recovery.should_restore(false, false, now));
        assert!(!recovery.should_restore(false, true, now + Duration::from_millis(300)));
        let unlock = now + Duration::from_secs(3600);
        assert!(!recovery.should_restore(false, true, unlock));
        assert!(!recovery.should_restore(false, false, unlock));
        assert!(!recovery.should_restore(false, false, unlock + RECONNECT_GRACE / 2));
        assert!(recovery.should_restore(false, false, unlock + RECONNECT_GRACE));
    }

    #[test]
    fn returning_tray_cancels_recovery_and_later_loss_gets_its_own_deadline() {
        let now = Instant::now();
        let mut recovery = TrayRecovery::default();
        assert!(!recovery.should_restore(false, false, now));
        assert!(!recovery.should_restore(true, false, now + Duration::from_secs(1)));
        assert!(!recovery.should_restore(true, false, now + RECONNECT_GRACE));
        let later = now + Duration::from_secs(60);
        assert!(!recovery.should_restore(false, false, later));
        assert!(recovery.should_restore(false, false, later + RECONNECT_GRACE));
    }
}
