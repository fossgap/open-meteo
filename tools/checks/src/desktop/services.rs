use super::common::*;
use anyhow::Result;
use glib::variant::ToVariant;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

pub fn run() -> Result<()> {
    let bus = session()?;
    let scheme = Rc::new(Cell::new(2u32));
    let locked = Rc::new(Cell::new(false));
    let items = Rc::new(RefCell::new(Vec::<String>::new()));
    let watcher = Rc::new(RefCell::new(Some(gio::bus_own_name_on_connection(
        &bus,
        WATCHER,
        gio::BusNameOwnerFlags::NONE,
        |_, _| {},
        |_, _| {},
    ))));
    let xml = r#"<node>
    <interface name="org.kde.StatusNotifierWatcher">
      <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
      <property name="ProtocolVersion" type="i" access="read"/>
      <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
      <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
      <signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
      <signal name="StatusNotifierItemUnregistered"><arg type="s"/></signal>
    </interface>
    <interface name="org.freedesktop.portal.Settings">
      <method name="Read"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
      <method name="ReadAll"><arg type="as" direction="in"/><arg type="a{sa{sv}}" direction="out"/></method>
      <method name="SetColorScheme"><arg type="u" direction="in"/></method>
      <method name="RemoveTray"/>
      <method name="RestoreTray"/>
      <method name="SetLocked"><arg type="b" direction="in"/></method>
      <property name="version" type="u" access="read"/>
      <signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
    </interface>
    <interface name="org.gnome.ScreenSaver">
      <method name="GetActive"><arg type="b" direction="out"/></method>
      <signal name="ActiveChanged"><arg type="b"/></signal>
    </interface></node>"#;
    let info = gio::DBusNodeInfo::for_xml(xml)?;
    let lost_items = items.clone();
    let signal_bus = bus.clone();
    let _owners = bus.subscribe_to_signal(
        Some("org.freedesktop.DBus"),
        Some("org.freedesktop.DBus"),
        Some("NameOwnerChanged"),
        Some("/org/freedesktop/DBus"),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let (name, _, owner): (String, String, String) = value(signal.parameters).unwrap();
            if !owner.is_empty() {
                return;
            }
            let removed = lost_items
                .borrow()
                .iter()
                .filter(|item| item.split('/').next() == Some(name.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            lost_items
                .borrow_mut()
                .retain(|item| !removed.contains(item));
            for item in removed {
                signal_bus
                    .emit_signal(
                        None,
                        "/StatusNotifierWatcher",
                        WATCHER,
                        "StatusNotifierItemUnregistered",
                        Some(&(item,).to_variant()),
                    )
                    .unwrap();
            }
        },
    );
    let registered = items.clone();
    let properties = items.clone();
    let _registration = bus
        .register_object("/StatusNotifierWatcher", &info.interfaces()[0])
        .method_call(move |connection, sender, path, _, _, args, invocation| {
            let item: String = first(&args).expect("Registration argument");
            let item = if item.starts_with('/') {
                format!("{}{item}", sender.unwrap())
            } else {
                format!("{item}/StatusNotifierItem")
            };
            if !registered.borrow().contains(&item) {
                registered.borrow_mut().push(item.clone());
            }
            invocation.return_value(Some(&().to_variant()));
            connection
                .emit_signal(
                    None,
                    path,
                    WATCHER,
                    "StatusNotifierItemRegistered",
                    Some(&(item,).to_variant()),
                )
                .expect("Registration signal");
        })
        .property(move |_, _, _, _, name| match name {
            "ProtocolVersion" => 0i32.to_variant(),
            "IsStatusNotifierHostRegistered" => true.to_variant(),
            "RegisteredStatusNotifierItems" => properties.borrow().to_variant(),
            _ => unreachable!(),
        })
        .build()?;
    let lock_state = locked.clone();
    let _screensaver = bus
        .register_object("/org/gnome/ScreenSaver", &info.interfaces()[2])
        .method_call(move |_, _, _, _, _, _, invocation| {
            invocation.return_value(Some(&(lock_state.get(),).to_variant()));
        })
        .build()?;
    let _screensaver_owner = gio::bus_own_name_on_connection(
        &bus,
        "org.gnome.ScreenSaver",
        gio::BusNameOwnerFlags::NONE,
        |_, _| {},
        |_, _| {},
    );
    let _settings = bus
        .register_object("/org/freedesktop/portal/desktop", &info.interfaces()[1])
        .method_call(
            move |connection, _, path, _, name, args, invocation| match name {
                "Read" => {
                    let (namespace, key): (String, String) = value(&args).unwrap();
                    if namespace == "org.freedesktop.appearance" && key == "color-scheme" {
                        invocation.return_value(Some(
                            &(scheme.get().to_variant().to_variant(),).to_variant(),
                        ));
                    } else {
                        invocation.return_dbus_error(
                            "org.freedesktop.portal.Error.NotFound",
                            "Setting not found",
                        );
                    }
                }
                "ReadAll" => invocation.return_value(Some(
                    &(BTreeMap::from([(
                        "org.freedesktop.appearance",
                        BTreeMap::from([("color-scheme", scheme.get().to_variant())]),
                    )]),)
                        .to_variant(),
                )),
                "SetColorScheme" => {
                    scheme.set(first(&args).unwrap());
                    connection
                        .emit_signal(
                            None,
                            path,
                            SETTINGS,
                            "SettingChanged",
                            Some(
                                &(
                                    "org.freedesktop.appearance",
                                    "color-scheme",
                                    scheme.get().to_variant(),
                                )
                                    .to_variant(),
                            ),
                        )
                        .unwrap();
                    invocation.return_value(Some(&().to_variant()));
                }
                "RemoveTray" => {
                    if let Some(owner) = watcher.borrow_mut().take() {
                        gio::bus_unown_name(owner);
                    }
                    invocation.return_value(Some(&().to_variant()));
                }
                "RestoreTray" => {
                    if watcher.borrow().is_none() {
                        *watcher.borrow_mut() = Some(gio::bus_own_name_on_connection(
                            &connection,
                            WATCHER,
                            gio::BusNameOwnerFlags::NONE,
                            |_, _| {},
                            |_, _| {},
                        ));
                    }
                    invocation.return_value(Some(&().to_variant()));
                }
                "SetLocked" => {
                    locked.set(first(&args).unwrap());
                    connection
                        .emit_signal(
                            None,
                            "/org/gnome/ScreenSaver",
                            "org.gnome.ScreenSaver",
                            "ActiveChanged",
                            Some(&(locked.get(),).to_variant()),
                        )
                        .unwrap();
                    invocation.return_value(Some(&().to_variant()));
                }
                _ => unreachable!(),
            },
        )
        .property(|_, _, _, _, _| 1u32.to_variant())
        .build()?;
    let _owner = gio::bus_own_name_on_connection(
        &bus,
        PORTAL,
        gio::BusNameOwnerFlags::NONE,
        |_, _| {},
        |_, _| {},
    );
    glib::MainLoop::new(None, false).run();
    Ok(())
}
