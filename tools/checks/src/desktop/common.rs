use crate::process::{run, text};
use anyhow::{Context, Result, bail, ensure};
use glib::{
    Variant,
    variant::{FromVariant, ToVariant},
};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    time::{Duration, Instant},
};

pub const PORTAL: &str = "org.freedesktop.portal.Desktop";
pub const SETTINGS: &str = "org.freedesktop.portal.Settings";
pub const WATCHER: &str = "org.kde.StatusNotifierWatcher";
pub const ITEM: &str = "org.kde.StatusNotifierItem";
pub const APP: &str = "io.github.open_meteo.OpenMeteo";
pub const DEMO: &str = "io.github.open_meteo.OpenMeteo.Demo";
pub const UUID: &str = "open-meteo-clock@fossgap.github.io";
pub type Environment = BTreeMap<String, String>;

pub fn call(
    bus: &gio::DBusConnection,
    service: &str,
    path: &str,
    interface: &str,
    method: &str,
    args: Option<&Variant>,
) -> Result<Variant> {
    bus.call_sync(
        Some(service),
        path,
        interface,
        method,
        args,
        None,
        gio::DBusCallFlags::NONE,
        3000,
        gio::Cancellable::NONE,
    )
    .with_context(|| format!("{service} {path} {interface}.{method}"))
}
pub fn value<T: FromVariant>(variant: &Variant) -> Result<T> {
    variant
        .get()
        .with_context(|| format!("Unexpected D-Bus value: {variant}"))
}
pub fn first<T: FromVariant>(reply: &Variant) -> Result<T> {
    value(&reply.child_value(0))
}
pub fn property(
    bus: &gio::DBusConnection,
    service: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> Result<Variant> {
    let reply = call(
        bus,
        service,
        path,
        "org.freedesktop.DBus.Properties",
        "Get",
        Some(&(interface, name).to_variant()),
    )?;
    reply
        .child_value(0)
        .as_variant()
        .context("D-Bus property variant")
}
pub fn session() -> Result<gio::DBusConnection> {
    Ok(gio::bus_get_sync(
        gio::BusType::Session,
        gio::Cancellable::NONE,
    )?)
}
pub fn has_name(bus: &gio::DBusConnection, name: &str) -> Result<bool> {
    first(&call(
        bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        Some(&(name,).to_variant()),
    )?)
}
pub fn pump() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}
pub fn pause(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        pump();
        std::thread::sleep(Duration::from_millis(10));
    }
}
pub fn wait<T>(description: &str, mut predicate: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(12);
    while Instant::now() < end {
        pump();
        if let Some(result) = predicate()? {
            return Ok(result);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    bail!("Timed out: {description}")
}
pub fn until(description: &str, mut predicate: impl FnMut() -> Result<bool>) -> Result<()> {
    wait(description, || Ok(predicate()?.then_some(())))
}

pub struct Process(pub Child);
impl Process {
    pub fn wait(&mut self) -> Result<ExitStatus> {
        wait("child process exits", || Ok(self.0.try_wait()?))
    }
    pub fn success(&mut self) -> Result<()> {
        ensure!(self.wait()?.success(), "Child process failed");
        Ok(())
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

pub fn command(program: impl AsRef<std::ffi::OsStr>, env: &Environment) -> Command {
    let mut command = Command::new(program);
    command.envs(env).env_remove("GTK_THEME");
    command
}
pub fn binary(root: &Path) -> Result<PathBuf> {
    Ok(std::env::var_os("OPEN_METEO_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/debug/open-meteo"))
        .canonicalize()?)
}
pub fn environment(private: &Path, desktop: &str) -> Environment {
    let mut env = Environment::from([
        ("GDK_BACKEND".into(), "x11".into()),
        ("GSK_RENDERER".into(), "cairo".into()),
        ("GIO_USE_VFS".into(), "local".into()),
        ("GTK_A11Y".into(), "atspi".into()),
        ("XDG_CURRENT_DESKTOP".into(), desktop.into()),
        ("LC_ALL".into(), "C.UTF-8".into()),
        ("LANGUAGE".into(), "en".into()),
        ("NO_PROXY".into(), "".into()),
    ]);
    for key in ["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"] {
        env.insert(key.into(), "http://127.0.0.1:9".into());
    }
    for (key, directory) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"),
    ] {
        env.insert(
            key.into(),
            private.join(directory).to_string_lossy().into_owned(),
        );
    }
    env
}

pub struct Harness {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub private: tempfile::TempDir,
    pub env: Environment,
    pub bus: gio::DBusConnection,
    pub log: File,
}
impl Harness {
    pub fn new(name: &str, desktop: &str) -> Result<Self> {
        let root = std::env::current_dir()?;
        let binary = binary(&root)?;
        let output = std::env::var_os("OPEN_METEO_TEST_OUTPUT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/ci").join(name));
        fs::create_dir_all(&output)?;
        let private = tempfile::Builder::new()
            .prefix("open-meteo-check-")
            .tempdir()?;
        let env = environment(private.path(), desktop);
        let log = File::create(output.join(format!("{name}.log")))?;
        Ok(Self {
            root,
            binary,
            output,
            private,
            env,
            bus: session()?,
            log,
        })
    }
    pub fn spawn(&self, args: &[&str]) -> Result<Process> {
        self.spawn_with(args, &self.env)
    }
    pub fn spawn_with(&self, args: &[&str], env: &Environment) -> Result<Process> {
        Ok(Process(
            command(&self.binary, env)
                .args(args)
                .stdout(self.log.try_clone()?)
                .stderr(self.log.try_clone()?)
                .spawn()?,
        ))
    }
    pub fn services(&self) -> Result<Process> {
        let helper = Process(
            command(std::env::current_exe()?, &self.env)
                .arg("services")
                .spawn()?,
        );
        until("test settings service", || has_name(&self.bus, PORTAL))?;
        Ok(helper)
    }
    pub fn setting(&self, method: &str, args: Option<&Variant>) -> Result<Variant> {
        call(
            &self.bus,
            PORTAL,
            "/org/freedesktop/portal/desktop",
            SETTINGS,
            method,
            args,
        )
    }
    pub fn registered(&self) -> Result<Vec<String>> {
        value(&property(
            &self.bus,
            WATCHER,
            "/StatusNotifierWatcher",
            WATCHER,
            "RegisteredStatusNotifierItems",
        )?)
    }
    pub fn bridge(&self, method: &str) -> Result<Variant> {
        call(
            &self.bus,
            DEMO,
            "/io/github/open_meteo/OpenMeteo/Weather",
            "io.github.open_meteo.OpenMeteo.Weather1",
            method,
            None,
        )
    }
}

pub fn xdo(args: &[&str]) -> Result<String> {
    Ok(text(Command::new("xdotool").args(args))?.trim().into())
}
pub fn window() -> Result<String> {
    xdo(&["search", "--name", "^Open Meteo$"])?
        .lines()
        .next()
        .map(str::to_owned)
        .context("Forecast window")
}
pub fn mapped(window: &str, state: &str) -> Result<bool> {
    Ok(text(Command::new("xwininfo").args(["-id", window]))?.contains(state))
}
pub fn geometry(window: &str) -> Result<BTreeMap<String, i32>> {
    xdo(&["getwindowgeometry", "--shell", window])?
        .lines()
        .map(|line| {
            let (key, value) = line.split_once('=').context("Window geometry")?;
            Ok((key.into(), value.parse()?))
        })
        .collect()
}
pub fn capture(name: &str, window: &str, output: &Path) -> Result<()> {
    xdo(&["windowmove", window, "0", "0"])?;
    xdo(&["mousemove", "0", "0"])?;
    pause(400);
    let g = geometry(window)?;
    run(Command::new("import")
        .args([
            "-window",
            "root",
            "-crop",
            &format!("{}x{}+{}+{}", g["WIDTH"], g["HEIGHT"], g["X"], g["Y"]),
        ])
        .arg(output.join(format!("{name}.png"))))?;
    Ok(())
}
pub fn action(bus: &gio::DBusConnection, demo: bool, name: &str) -> Result<()> {
    let (service, path) = if demo {
        (DEMO, "/io/github/open_meteo/OpenMeteo/Demo")
    } else {
        (APP, "/io/github/open_meteo/OpenMeteo")
    };
    call(
        bus,
        service,
        path,
        "org.gtk.Actions",
        "Activate",
        Some(
            &(
                name,
                Vec::<Variant>::new(),
                BTreeMap::<String, Variant>::new(),
            )
                .to_variant(),
        ),
    )?;
    Ok(())
}
pub fn read_json(path: impl AsRef<Path>) -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub fn write_json(path: impl AsRef<Path>, value: &serde_json::Value) -> Result<()> {
    fs::write(path, serde_json::to_vec(value)?)?;
    Ok(())
}
