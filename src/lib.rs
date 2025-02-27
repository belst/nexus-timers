use nexus::gui::{register_render, render, RenderType};
use nexus::imgui::{Ui, Window};
use nexus::keybind::{register_keybind_with_string, unregister_keybind};
use nexus::paths::get_addon_dir;
use nexus::{keybind_handler, localization::set_translation, AddonFlags, UpdateProvider};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

fn config_path() -> PathBuf {
    get_addon_dir("timers")
        .expect("Addon dir to exist")
        .join("timers.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Timer {
    name: String,
    duration: Duration,
    // If None, start and stop is the same
    #[serde(skip, default)]
    started: Option<Instant>,
}

impl Timer {
    fn new(name: String, duration: Duration) -> Self {
        let new = Self {
            name,
            duration,
            started: None,
        };

        new.register_keybind();
        new.register_localization();
        new
    }

    fn find_by_name<'a>(timers: &'a mut Vec<Self>, name: &'_ str) -> Option<&'a mut Self> {
        timers.iter_mut().find(|t| t.name == name)
    }

    const LANGS: &[&str] = &["br", "cn", "cz", "de", "en", "es", "fr", "it", "pl", "ru"];
    fn register_localization(&self) {
        for &l in Self::LANGS {
            self.localize(l);
        }
    }

    fn localize(&self, lang: &str) {
        set_translation(format!("KB_TIMER_START_{}", self.name), lang, &self.name);
    }

    fn register_keybind(&self) {
        let start_key_handler = keybind_handler!(|id, is_release| {
            if is_release {
                return;
            }
            let name = id.trim_start_matches("KB_TIMER_START_");
            let mut timers = TIMERS.get().expect("Timers to be set").lock().unwrap();
            if let Some(timer) = Timer::find_by_name(&mut *timers, name) {
                timer.started = Some(std::time::Instant::now());
            }
        });
        let _ = register_keybind_with_string(
            format!("KB_TIMER_START_{}", self.name),
            start_key_handler,
            "(null)",
        );
        self.register_localization();
    }

    fn unregister_keybind(&self) {
        unregister_keybind(format!("KB_TIMER_START_{}", self.name));
    }
}

static TIMERS: std::sync::OnceLock<Mutex<Vec<Timer>>> = std::sync::OnceLock::new();

fn load() {
    log::info!("Loading timers");
    let config: Vec<Timer> = if let Ok(f) = std::fs::File::open(config_path()) {
        match serde_json::from_reader(f) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to parse timers.json: {}", e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    for timer in &config {
        timer.register_keybind();
        log::info!("Loaded timer {}", timer.name);
    }
    TIMERS
        .set(Mutex::new(config))
        .expect("Timers to be set only once");

    register_render(RenderType::Render, render!(render_fn)).revert_on_unload();
    register_render(RenderType::OptionsRender, render!(render_options)).revert_on_unload();
}

fn render_fn(ui: &Ui) {
    let timers = TIMERS.get().expect("Timers to be set").lock().unwrap();
    for timer in timers.iter().filter(|t| t.started.is_some()) {
        let started = timer.started.expect("Timer to have started");
        let elapsed = started.elapsed();
        let rest = if elapsed >= timer.duration {
            // action on timer finish?
            0.0
        } else {
            (timer.duration - elapsed).as_secs_f32()
        };
        Window::new(timer.name.as_str()).build(ui, || ui.text(format!("{:.2}", rest)));
    }
}

fn render_options(ui: &Ui) {
    let mut timers = TIMERS.get().expect("Timers to be set").lock().unwrap();
    let mut to_remove = Vec::new();
    if let Some(_tbl) = ui.begin_table("timer_options", 3) {
        for (idx, timer) in timers.iter_mut().enumerate() {
            ui.table_next_row();
            ui.table_next_column();
            ui.text(timer.name.as_str());
            ui.table_next_column();
            let mut seconds = timer.duration.as_secs() as i32;
            ui.input_int(format!("{:?}", seconds), &mut seconds)
                .read_only(timer.started.is_some())
                .build();
            if seconds >= 0 {
                timer.duration = std::time::Duration::from_secs(seconds as u64);
            }
            ui.table_next_column();
            if ui.button("Delete") {
                to_remove.push(idx);
                timer.unregister_keybind();
            }
        }
        let tmp_timers = std::mem::take(&mut *timers);
        *timers = tmp_timers
            .into_iter()
            .enumerate()
            .filter(|(idx, _)| !to_remove.contains(idx))
            .map(|(_, t)| t)
            .collect();
        ui.table_next_row();
        ui.table_next_column();
        thread_local! {
            static NEW_NAME: RefCell<String> = const { RefCell::new(String::new()) };
            static NEW_DURATION: Cell<i32> = const { Cell::new(0) };
        }
        NEW_NAME.with_borrow_mut(|nn| {
            ui.input_text("Name", nn).build();
        });
        ui.table_next_column();
        let mut new_duration = NEW_DURATION.get();
        ui.input_int("Seconds", &mut new_duration).build();
        NEW_DURATION.set(new_duration);
        ui.table_next_column();
        if ui.button("Add") {
            NEW_NAME.with_borrow(|nn| {
                if nn.is_empty() {
                    return;
                }
                timers.push(Timer::new(
                    NEW_NAME.replace(String::new()),
                    Duration::from_secs(NEW_DURATION.get() as u64),
                ));
                NEW_DURATION.set(0);
            })
        }
    }
}

fn unload() {
    log::info!("Unloading timers");
    let timers = TIMERS.get().expect("Timers to be set").lock().unwrap();
    let json = serde_json::to_string_pretty(&*timers).expect("Timers to be serialized");
    let config = config_path();
    log::info!("Saving timers to {}", config.display());
    let _ = std::fs::create_dir_all(&config.parent().unwrap());
    let _ = std::fs::write(&config, json);
}

nexus::export! {
    name: "Timers",
    signature: -69422,
    flags: AddonFlags::None,
    load,
    unload,
    provider: UpdateProvider::GitHub,
    update_link: "https://github.com/belst/nexus-timers",
    log_filter: "warn,timers=debug"
}
