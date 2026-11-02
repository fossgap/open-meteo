//! Safe, direct AT-SPI D-Bus calls for native controls.
//! Protocol: https://gitlab.gnome.org/GNOME/at-spi2-core/-/tree/main/xml
use super::common::*;
use anyhow::{Context, Result, ensure};
use glib::{
    Variant,
    variant::{ObjectPath, ToVariant},
};
use std::collections::BTreeSet;

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";

// Stable AT-SPI role IDs. Toolkit-provided role names differ between GTK
// versions (for example, "radio menu item" versus "menu item radio").
#[derive(Clone, Copy)]
pub enum Role {
    MenuItem = 35,
    Button = 43,
    RadioMenuItem = 45,
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
impl Rect {
    pub fn crop(self) -> String {
        format!("{}x{}+{}+{}", self.width, self.height, self.x, self.y)
    }
}

#[derive(Clone)]
pub struct Node {
    bus: gio::DBusConnection,
    service: String,
    path: String,
    pub name: String,
    pub application: String,
}
impl Node {
    fn call(&self, interface: &str, method: &str, args: Option<&Variant>) -> Result<Variant> {
        call(
            &self.bus,
            &self.service,
            &self.path,
            interface,
            method,
            args,
        )
    }
    pub fn role(&self) -> Result<String> {
        first(&self.call(ACCESSIBLE, "GetRoleName", None)?)
    }
    pub fn is_role(&self, role: Role) -> bool {
        self.call(ACCESSIBLE, "GetRole", None)
            .and_then(|v| first::<u32>(&v))
            .is_ok_and(|value| value == role as u32)
    }
    pub fn actions(&self) -> i32 {
        property(
            &self.bus,
            &self.service,
            &self.path,
            "org.a11y.atspi.Action",
            "NActions",
        )
        .ok()
        .and_then(|v| v.get())
        .unwrap_or(0)
    }
    pub fn activate(&self) -> Result<()> {
        let worked: bool = first(&self.call(
            "org.a11y.atspi.Action",
            "DoAction",
            Some(&(0i32,).to_variant()),
        )?)?;
        ensure!(worked, "Could not activate {}", self.name);
        Ok(())
    }
    pub fn rect(&self) -> Result<Rect> {
        let (x, y, width, height): (i32, i32, i32, i32) = first(&self.call(
            "org.a11y.atspi.Component",
            "GetExtents",
            Some(&(0u32,).to_variant()),
        )?)?;
        Ok(Rect {
            x,
            y,
            width,
            height,
        })
    }
    pub fn showing(&self) -> Result<bool> {
        // ATSPI_STATE_SHOWING is bit 25 in the two-u32 state bitset.
        let state: Vec<u32> = first(&self.call(ACCESSIBLE, "GetState", None)?)?;
        Ok(state.first().is_some_and(|bits| bits & (1 << 25) != 0))
    }
}

pub struct Accessibility(gio::DBusConnection);
impl Accessibility {
    pub fn new() -> Result<Self> {
        let address: String = first(&call(
            &session()?,
            "org.a11y.Bus",
            "/org/a11y/bus",
            "org.a11y.Bus",
            "GetAddress",
            None,
        )?)?;
        let bus = gio::DBusConnection::for_address_sync(
            &address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            gio::Cancellable::NONE,
        )?;
        Ok(Self(bus))
    }
    pub fn nodes(&self) -> Vec<Node> {
        let mut nodes = Vec::new();
        let mut visited = BTreeSet::new();
        let mut stack = vec![(
            "org.a11y.atspi.Registry".to_owned(),
            "/org/a11y/atspi/accessible/root".to_owned(),
            String::new(),
        )];
        while let Some((service, path, application)) = stack.pop() {
            if !visited.insert((service.clone(), path.clone())) {
                continue;
            }
            let Ok(name) = property(&self.0, &service, &path, ACCESSIBLE, "Name")
                .and_then(|v| value::<String>(&v))
            else {
                continue;
            };
            let node = Node {
                bus: self.0.clone(),
                service,
                path,
                name,
                application,
            };
            let application =
                if node.application.is_empty() && node.service != "org.a11y.atspi.Registry" {
                    node.name.clone()
                } else {
                    node.application.clone()
                };
            // Match libatspi's indexed traversal. Older ATK bridges omit
            // embedded GtkSocket/AtkPlug children from GetChildren (XFCE's
            // external tray plugin), but expose them through ChildCount and
            // GetChildAtIndex.
            if let Ok(count) =
                property(&self.0, &node.service, &node.path, ACCESSIBLE, "ChildCount")
                    .and_then(|v| value::<i32>(&v))
            {
                for index in (0..count).rev() {
                    if let Ok((service, path)) = node
                        .call(ACCESSIBLE, "GetChildAtIndex", Some(&(index,).to_variant()))
                        .and_then(|v| first::<(String, ObjectPath)>(&v))
                    {
                        stack.push((service, path.as_str().to_owned(), application.clone()));
                    }
                }
            }
            nodes.push(Node {
                application,
                ..node
            });
        }
        nodes
    }
    pub fn find(&self, name: &str) -> Option<Node> {
        self.nodes().into_iter().find(|n| n.name == name)
    }
    pub fn present(&self, text: &str) -> bool {
        self.nodes().iter().any(|n| n.name.contains(text))
    }
    pub fn expect(&self, name: &str) -> Result<Node> {
        wait(&format!("control {name}"), || Ok(self.find(name)))
    }
    pub fn text(&self, text: &str) -> Result<()> {
        until(text, || Ok(self.present(text)))
    }
    pub fn click(&self, name: &str) -> Result<()> {
        if std::env::var_os("OPEN_METEO_TEST_TRACE").is_some() {
            eprintln!("Activate: {name}");
        }
        let node = wait(&format!("control {name}"), || {
            let nodes = self.nodes();
            Ok(nodes
                .iter()
                .find(|n| n.name == name && n.actions() > 0)
                .cloned()
                .or_else(|| nodes.into_iter().find(|n| n.name == name)))
        })
        .inspect_err(|_| {
            for node in self.nodes() {
                eprintln!(
                    "Accessible {:?}: {:?} ({:?})",
                    node.application,
                    node.name,
                    node.role()
                );
            }
        })?;
        if node.actions() > 0 {
            node.activate()
        } else {
            let rect = node.rect().context("Clickable rectangle")?;
            xdo(&[
                "mousemove",
                &(rect.x + rect.width / 2).to_string(),
                &(rect.y + rect.height / 2).to_string(),
            ])?;
            xdo(&["click", "1"])?;
            Ok(())
        }
    }
}
