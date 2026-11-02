use super::common::*;
use crate::process::text;
use anyhow::{Result, ensure};
use glib::variant::ToVariant;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    io::Read,
    process::Stdio,
    rc::Rc,
    time::Duration,
};

#[derive(Clone)]
struct Scenario {
    unknown: bool,
    state: f64,
    version: f64,
    shell_version: String,
    disabled: bool,
    refuse: bool,
    broken: bool,
    enable_calls: u32,
}

pub fn run() -> Result<()> {
    let root = std::env::current_dir()?;
    let source = root.join("resources/gnome-extension");
    let version = read_json(source.join("metadata.json"))?["version"]
        .as_f64()
        .unwrap();
    let default = Scenario {
        unknown: false,
        state: 2.0,
        version,
        shell_version: "50.4".into(),
        disabled: false,
        refuse: false,
        broken: false,
        enable_calls: 0,
    };
    let scenario = Rc::new(RefCell::new(default.clone()));
    let bus = session()?;
    let xml = r#"<node><interface name="org.gnome.Shell.Extensions">
    <method name="GetExtensionInfo"><arg type="s" direction="in"/><arg type="a{sv}" direction="out"/></method>
    <method name="EnableExtension"><arg type="s" direction="in"/><arg type="b" direction="out"/></method>
    <property name="ShellVersion" type="s" access="read"/>
    <property name="UserExtensionsEnabled" type="b" access="read"/>
    </interface></node>"#;
    let info = gio::DBusNodeInfo::for_xml(xml)?;
    let state = scenario.clone();
    let properties = scenario.clone();
    let extension_source = source.clone();
    let registration = bus
        .register_object("/org/gnome/Shell", &info.interfaces()[0])
        .method_call(move |_, _, _, _, name, args, invocation| {
            assert_eq!(first::<String>(&args).unwrap(), UUID);
            let mut s = state.borrow_mut();
            match name {
                "GetExtensionInfo" => {
                    let data = if s.unknown {
                        BTreeMap::new()
                    } else {
                        BTreeMap::from([
                            ("path", extension_source.to_string_lossy().to_variant()),
                            ("state", s.state.to_variant()),
                            ("version", s.version.to_variant()),
                        ])
                    };
                    invocation.return_value(Some(&(data,).to_variant()));
                }
                "EnableExtension" => {
                    s.enable_calls += 1;
                    invocation.return_value(Some(&(!s.refuse,).to_variant()));
                    if !s.refuse {
                        let state = state.clone();
                        glib::timeout_add_local_once(Duration::from_millis(80), move || {
                            let mut s = state.borrow_mut();
                            s.state = if s.broken { 3.0 } else { 1.0 };
                        });
                    }
                }
                _ => unreachable!(),
            }
        })
        .property(move |_, _, _, _, name| {
            let s = properties.borrow();
            match name {
                "ShellVersion" => s.shell_version.to_variant(),
                "UserExtensionsEnabled" => (!s.disabled).to_variant(),
                _ => unreachable!(),
            }
        })
        .build()?;
    let acquired = Rc::new(Cell::new(false));
    let ready = acquired.clone();
    let owner = gio::bus_own_name_on_connection(
        &bus,
        "org.gnome.Shell",
        gio::BusNameOwnerFlags::NONE,
        move |_, _| ready.set(true),
        |_, _| {},
    );
    until("private GNOME service", || Ok(acquired.get()))?;
    let directory = tempfile::tempdir()?;
    let private = directory.path();
    let schema = private.join("share/glib-2.0/schemas");
    fs::create_dir_all(&schema)?;
    fs::write(
        schema.join("org.gnome.shell.gschema.xml"),
        r#"<schemalist>
    <schema id="org.gnome.shell" path="/org/gnome/shell/">
    <key name="enabled-extensions" type="as"><default>['other@example']</default></key>
    <key name="disabled-extensions" type="as"><default>['disabled@example', 'open-meteo-clock@fossgap.github.io']</default></key>
    </schema></schemalist>"#,
    )?;
    crate::process::run(std::process::Command::new("glib-compile-schemas").arg(&schema))?;
    let executable = binary(&root)?;
    let check =
        |name: &str, expected: &str, success: bool, selected: Scenario| -> Result<Environment> {
            *scenario.borrow_mut() = selected;
            let env = Environment::from([
                (
                    "XDG_CONFIG_HOME".into(),
                    private
                        .join(name)
                        .join("config")
                        .to_string_lossy()
                        .into_owned(),
                ),
                (
                    "XDG_DATA_HOME".into(),
                    private
                        .join(name)
                        .join("data")
                        .to_string_lossy()
                        .into_owned(),
                ),
                ("GSETTINGS_BACKEND".into(), "keyfile".into()),
                (
                    "GSETTINGS_SCHEMA_DIR".into(),
                    schema.to_string_lossy().into_owned(),
                ),
                ("LANGUAGE".into(), "en".into()),
                ("LC_ALL".into(), "C.UTF-8".into()),
                (
                    "XDG_DATA_DIRS".into(),
                    format!(
                        "{}:/usr/local/share:/usr/share",
                        private.join("share").display()
                    ),
                ),
            ]);
            if !std::path::Path::new(&env["XDG_CONFIG_HOME"]).exists() {
                for (key, value) in [
                    ("enabled-extensions", "['other@example']"),
                    (
                        "disabled-extensions",
                        "['disabled@example', 'open-meteo-clock@fossgap.github.io']",
                    ),
                ] {
                    text(command("gsettings", &env).args(["set", "org.gnome.shell", key, value]))?;
                }
            }
            let mut process = Process(
                command(&executable, &env)
                    .arg("--install-gnome-extension")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?,
            );
            let status = process.wait()?;
            let mut output = String::new();
            process
                .0
                .stdout
                .take()
                .unwrap()
                .read_to_string(&mut output)?;
            process
                .0
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut output)?;
            ensure!(
                status.success() == success && output.contains(expected),
                "Installer {name}: {output}"
            );
            for entry in fs::read_dir(&source)? {
                let entry = entry?;
                ensure!(
                    fs::read(
                        std::path::Path::new(&env["XDG_DATA_HOME"])
                            .join("gnome-shell/extensions")
                            .join(UUID)
                            .join(entry.file_name())
                    )? == fs::read(entry.path())?,
                    "Installer file differs: {:?}",
                    entry.file_name()
                );
            }
            Ok(env)
        };
    check(
        "enable",
        "GNOME clock integration is enabled.",
        true,
        default.clone(),
    )?;
    ensure!(
        scenario.borrow().state == 1.0 && scenario.borrow().enable_calls == 1,
        "Enable once"
    );
    check(
        "already-active",
        "GNOME clock integration is enabled.",
        true,
        Scenario {
            state: 1.0,
            ..default.clone()
        },
    )?;
    let env = check(
        "first-install",
        "It will enable automatically.",
        true,
        Scenario {
            unknown: true,
            ..default.clone()
        },
    )?;
    ensure!(
        scenario.borrow().enable_calls == 0,
        "Unknown extension cannot be enabled by this API"
    );
    let setting = |key| text(command("gsettings", &env).args(["get", "org.gnome.shell", key]));
    ensure!(
        setting("enabled-extensions")?.contains(UUID)
            && setting("enabled-extensions")?.contains("other@example"),
        "Preserve enabled extensions"
    );
    ensure!(
        !setting("disabled-extensions")?.contains(UUID)
            && setting("disabled-extensions")?.contains("disabled@example"),
        "Preserve disabled extensions"
    );
    check(
        "first-install",
        "It will enable automatically.",
        true,
        Scenario {
            unknown: true,
            ..default.clone()
        },
    )?;
    ensure!(
        setting("enabled-extensions")?.matches(UUID).count() == 1,
        "Setup must be idempotent"
    );
    check(
        "update",
        "has been updated",
        true,
        Scenario {
            version: version - 1.0,
            ..default.clone()
        },
    )?;
    ensure!(
        scenario.borrow().enable_calls == 0,
        "Cached code must not be reported updated live"
    );
    check(
        "disabled",
        "GNOME extensions are switched off.",
        false,
        Scenario {
            disabled: true,
            ..default.clone()
        },
    )?;
    ensure!(
        scenario.borrow().enable_calls == 0,
        "Do not enable globally disabled extensions"
    );
    check(
        "unsupported",
        "not supported",
        false,
        Scenario {
            shell_version: "99.0".into(),
            ..default.clone()
        },
    )?;
    check(
        "refused",
        "GNOME did not enable",
        false,
        Scenario {
            refuse: true,
            ..default.clone()
        },
    )?;
    check(
        "broken",
        "GNOME did not enable",
        false,
        Scenario {
            broken: true,
            ..default.clone()
        },
    )?;
    gio::bus_unown_name(owner);
    pause(200);
    check(
        "no-shell",
        "Run clock setup from a GNOME session",
        true,
        default,
    )?;
    bus.unregister_object(registration)?;
    println!(
        "GNOME setup checks passed: live enable, already active, first install, idempotency, updates, disabled extensions, unsupported versions, refused activation, broken extension, and no Shell."
    );
    Ok(())
}
