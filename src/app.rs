use crate::{
    audio::{
        list_input_devices, list_output_devices, spawn_audio_engine, AudioCommand,
        AudioEngineHandle, AudioSettings, AudioStatus,
    },
    meter::{MeterData, PeakHold},
    params::{self, Direction, FilterMode, Params},
};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde::{Deserialize, Serialize};
use std::{f32::consts::TAU, sync::Arc};

const APP_KEY: &str = "envelope-filter-app-state";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedState {
    sensitivity: f32,
    freq_min_hz: f32,
    freq_max_hz: f32,
    resonance: f32,
    attack_ms: f32,
    release_ms: f32,
    filter_mode: u32,
    direction: u32,
    pre_comp_on: bool,
    comp_threshold_db: f32,
    comp_ratio: f32,
    makeup_db: f32,
    dry_wet_percent: f32,
    input_device_name: Option<String>,
    output_device_name: Option<String>,
    main_channel: usize,
    sample_rate_hz: u32,
    buffer_size: u32,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            sensitivity: 0.65,
            freq_min_hz: 250.0,
            freq_max_hz: 2500.0,
            resonance: 3.5,
            attack_ms: 6.0,
            release_ms: 100.0,
            filter_mode: FilterMode::BandPass as u32,
            direction: Direction::Up as u32,
            pre_comp_on: true,
            comp_threshold_db: -16.0,
            comp_ratio: 8.0,
            makeup_db: 0.0,
            dry_wet_percent: 100.0,
            input_device_name: None,
            output_device_name: None,
            main_channel: 0,
            sample_rate_hz: 48_000,
            buffer_size: 256,
        }
    }
}

pub struct DuckerApp {
    params: Arc<Params>,
    state: PersistedState,
    engine: AudioEngineHandle,
    status: AudioStatus,
    meter_data: MeterData,
    input_peak_hold: PeakHold,
    envelope_hold: PeakHold,
    output_peak_hold: PeakHold,
    last_button_click: std::time::Instant,
}

impl DuckerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = cc
            .storage
            .and_then(|s| eframe::get_value::<PersistedState>(s, APP_KEY))
            .unwrap_or_default();

        let params = Arc::new(Params::default());
        params.set_sensitivity(state.sensitivity);
        params.set_freq_min(state.freq_min_hz);
        params.set_freq_max(state.freq_max_hz);
        params.set_resonance(state.resonance);
        params.set_attack_ms(state.attack_ms);
        params.set_release_ms(state.release_ms);
        params.set_filter_mode(FilterMode::from_u32(state.filter_mode));
        params.set_direction(Direction::from_u32(state.direction));
        params.set_pre_comp_on(state.pre_comp_on);
        params.set_comp_threshold(state.comp_threshold_db);
        params.set_comp_ratio(state.comp_ratio);
        params.set_makeup_db(state.makeup_db);
        params.set_dry_wet_percent(state.dry_wet_percent);

        let engine = spawn_audio_engine(
            Arc::clone(&params),
            AudioSettings {
                input_device_name: state.input_device_name.clone(),
                output_device_name: state.output_device_name.clone(),
                main_channel: state.main_channel,
                sample_rate_hz: Some(state.sample_rate_hz),
                buffer_size: Some(state.buffer_size),
            },
        );

        Self {
            params,
            state,
            engine,
            status: AudioStatus::Stopped,
            meter_data: MeterData::default(),
            input_peak_hold: PeakHold::new(),
            envelope_hold: PeakHold::new(),
            output_peak_hold: PeakHold::new(),
            last_button_click: std::time::Instant::now(),
        }
    }

    fn restart_audio(&self) {
        let _ = self
            .engine
            .command_tx
            .try_send(AudioCommand::Restart(AudioSettings {
                input_device_name: self.state.input_device_name.clone(),
                output_device_name: self.state.output_device_name.clone(),
                main_channel: self.state.main_channel,
                sample_rate_hz: Some(self.state.sample_rate_hz),
                buffer_size: Some(self.state.buffer_size),
            }));
    }

    fn can_click_button(&mut self) -> bool {
        const DEBOUNCE_MS: u128 = 50;
        let elapsed = self.last_button_click.elapsed().as_millis();
        if elapsed >= DEBOUNCE_MS {
            self.last_button_click = std::time::Instant::now();
            true
        } else {
            false
        }
    }

    fn knob(
        ui: &mut egui::Ui,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
        label: &str,
        suffix: &str,
        enabled: bool,
    ) -> bool {
        let desired = Vec2::new(84.0, 112.0);
        let (rect, mut response) = ui.allocate_exact_size(desired, Sense::drag());

        if enabled && response.dragged() {
            let span = *range.end() - *range.start();
            let delta = -response.drag_delta().y * span / 300.0;
            let next = (*value + delta).clamp(*range.start(), *range.end());
            if (next - *value).abs() > f32::EPSILON {
                *value = next;
                response.mark_changed();
            }
        }

        let painter = ui.painter_at(rect);
        let center = Pos2::new(rect.center().x, rect.top() + 44.0);
        let radius = 28.0;
        let accent = if enabled {
            Color32::from_rgb(0x00, 0xAA, 0xFF)
        } else {
            Color32::from_gray(60)
        };
        let text_color = if enabled {
            Color32::LIGHT_GRAY
        } else {
            Color32::from_gray(80)
        };
        painter.circle_filled(center, radius, Color32::from_rgb(0x24, 0x24, 0x24));
        painter.circle_stroke(center, radius, Stroke::new(1.5, Color32::from_gray(90)));

        let t = (*value - *range.start()) / (*range.end() - *range.start());
        let start_angle = -0.75 * TAU / 2.0;
        let end_angle = 0.75 * TAU / 2.0;
        let angle = start_angle + (end_angle - start_angle) * t;

        let mut points = Vec::new();
        let steps = 32;
        for i in 0..=steps {
            let a = start_angle + (angle - start_angle) * (i as f32 / steps as f32);
            points.push(Pos2::new(
                center.x + a.cos() * (radius - 4.0),
                center.y + a.sin() * (radius - 4.0),
            ));
        }
        painter.add(egui::Shape::line(points, Stroke::new(3.0, accent)));

        let dot = Pos2::new(
            center.x + angle.cos() * (radius - 8.0),
            center.y + angle.sin() * (radius - 8.0),
        );
        let dot_color = if enabled {
            Color32::WHITE
        } else {
            Color32::from_gray(100)
        };
        painter.circle_filled(dot, 3.5, dot_color);

        painter.text(
            Pos2::new(rect.center().x, rect.top() + 4.0),
            egui::Align2::CENTER_TOP,
            format!("{value:.1}{suffix}"),
            egui::FontId::proportional(13.0),
            text_color,
        );
        let label_color = if enabled {
            Color32::from_gray(180)
        } else {
            Color32::from_gray(80)
        };
        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 8.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(12.0),
            label_color,
        );

        response.changed()
    }

    fn meter_bar(
        ui: &mut egui::Ui,
        label: &str,
        db: f32,
        peak_db: f32,
        accent: Color32,
    ) {
        let size = Vec2::new(48.0, 180.0);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_rgb(0x11, 0x11, 0x11));
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, Color32::from_gray(80)),
            StrokeKind::Outside,
        );

        let bottom = rect.bottom() - 20.0;
        let top = rect.top() + 8.0;
        let height = bottom - top;

        let norm = ((db + 60.0) / 60.0).clamp(0.0, 1.0);

        let fill_top = bottom - (height * norm);
        let fill_rect = Rect::from_min_max(
            Pos2::new(rect.left() + 8.0, fill_top),
            Pos2::new(rect.right() - 8.0, bottom),
        );
        painter.rect_filled(fill_rect, 2.0, accent);

        let hold_norm = ((peak_db + 60.0) / 60.0).clamp(0.0, 1.0);
        let hold_y = bottom - (height * hold_norm);
        painter.line_segment(
            [
                Pos2::new(rect.left() + 8.0, hold_y),
                Pos2::new(rect.right() - 8.0, hold_y),
            ],
            Stroke::new(2.0, Color32::WHITE),
        );

        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(11.0),
            Color32::from_gray(180),
        );
    }

    fn envelope_meter(ui: &mut egui::Ui, label: &str, value: f32, accent: Color32) {
        let size = Vec2::new(48.0, 180.0);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_rgb(0x11, 0x11, 0x11));
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, Color32::from_gray(80)),
            StrokeKind::Outside,
        );

        let bottom = rect.bottom() - 20.0;
        let top = rect.top() + 8.0;
        let height = bottom - top;

        let norm = value.clamp(0.0, 1.0);
        let fill_top = bottom - (height * norm);
        let fill_rect = Rect::from_min_max(
            Pos2::new(rect.left() + 8.0, fill_top),
            Pos2::new(rect.right() - 8.0, bottom),
        );
        painter.rect_filled(fill_rect, 2.0, accent);

        painter.text(
            Pos2::new(rect.center().x, rect.top() + 2.0),
            egui::Align2::CENTER_TOP,
            format!("{:.0}%", norm * 100.0),
            egui::FontId::proportional(10.0),
            Color32::LIGHT_GRAY,
        );

        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(11.0),
            Color32::from_gray(180),
        );
    }

    fn cutoff_meter(
        ui: &mut egui::Ui,
        label: &str,
        cutoff_hz: f32,
        freq_min: f32,
        freq_max: f32,
        accent: Color32,
    ) {
        let size = Vec2::new(48.0, 180.0);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_rgb(0x11, 0x11, 0x11));
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, Color32::from_gray(80)),
            StrokeKind::Outside,
        );

        let bottom = rect.bottom() - 20.0;
        let top = rect.top() + 8.0;
        let height = bottom - top;

        let log_min = freq_min.max(1.0).ln();
        let log_max = freq_max.max(freq_min + 1.0).ln();
        let log_cut = cutoff_hz.clamp(freq_min, freq_max).ln();
        let norm = if (log_max - log_min).abs() > f32::EPSILON {
            ((log_cut - log_min) / (log_max - log_min)).clamp(0.0, 1.0)
        } else {
            0.5
        };

        let fill_top = bottom - (height * norm);
        let fill_rect = Rect::from_min_max(
            Pos2::new(rect.left() + 8.0, fill_top),
            Pos2::new(rect.right() - 8.0, bottom),
        );
        painter.rect_filled(fill_rect, 2.0, accent);

        painter.text(
            Pos2::new(rect.center().x, rect.top() + 2.0),
            egui::Align2::CENTER_TOP,
            format!("{:.0} Hz", cutoff_hz),
            egui::FontId::proportional(10.0),
            Color32::LIGHT_GRAY,
        );

        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(11.0),
            Color32::from_gray(180),
        );
    }
}

impl eframe::App for DuckerApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, APP_KEY, &self.state);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut had_update = false;

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(20, 18, 36);
        visuals.widgets.active.bg_fill = Color32::from_rgb(255, 184, 46);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(255, 206, 92);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(66, 56, 102);
        visuals.widgets.active.fg_stroke.color = Color32::from_rgb(25, 22, 35);
        visuals.widgets.hovered.fg_stroke.color = Color32::from_rgb(25, 22, 35);
        ctx.set_visuals(visuals);

        while let Ok(status) = self.engine.status_rx.try_recv() {
            self.status = status;
            had_update = true;
        }

        while let Ok(meter) = self.engine.meter_rx.try_recv() {
            self.meter_data = meter;
            had_update = true;
        }

        if had_update {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }

        self.input_peak_hold.update(self.meter_data.input_peak_db);
        let env_db = if self.meter_data.envelope > 1e-6 {
            20.0 * self.meter_data.envelope.log10()
        } else {
            -60.0
        };
        self.envelope_hold.update(env_db);
        self.output_peak_hold.update(self.meter_data.output_peak_db);

        egui::TopBottomPanel::top("title").show(&ctx, |ui| {
            ui.set_height(92.0);
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("ENVELOPE FILTER")
                        .monospace()
                        .size(28.0)
                        .color(Color32::from_rgb(255, 214, 64))
                        .strong(),
                );
                ui.add_space(8.0);

                let status_color = match &self.status {
                    AudioStatus::Running => Color32::LIGHT_GREEN,
                    AudioStatus::Stopped => Color32::RED,
                    AudioStatus::Error(_) => Color32::YELLOW,
                };
                let status_text = match &self.status {
                    AudioStatus::Running => "RUNNING",
                    AudioStatus::Stopped => "STOPPED",
                    AudioStatus::Error(_) => "ERROR",
                };
                ui.colored_label(status_color, status_text);
            });

            ui.horizontal_wrapped(|ui| {
                let input_devices = list_input_devices();
                let output_devices = list_output_devices();

                let selected_input = self
                    .state
                    .input_device_name
                    .clone()
                    .unwrap_or_else(|| "Default Input".to_string());
                egui::ComboBox::from_label("Input Device")
                    .selected_text(selected_input)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                self.state.input_device_name.is_none(),
                                "Default Input",
                            )
                            .clicked()
                        {
                            self.state.input_device_name = None;
                            self.restart_audio();
                        }
                        for name in &input_devices {
                            if ui
                                .selectable_label(
                                    self.state.input_device_name.as_deref() == Some(name),
                                    name,
                                )
                                .clicked()
                            {
                                self.state.input_device_name = Some(name.clone());
                                self.restart_audio();
                            }
                        }
                    });

                egui::ComboBox::from_label("Main Ch")
                    .selected_text(format!("{}", self.state.main_channel + 1))
                    .show_ui(ui, |ui| {
                        for ch in 0..16usize {
                            if ui
                                .selectable_label(
                                    self.state.main_channel == ch,
                                    format!("{}", ch + 1),
                                )
                                .clicked()
                            {
                                self.state.main_channel = ch;
                                self.restart_audio();
                            }
                        }
                    });

                let selected_output = self
                    .state
                    .output_device_name
                    .clone()
                    .unwrap_or_else(|| "Default Output".to_string());
                egui::ComboBox::from_label("Output Device")
                    .selected_text(selected_output)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                self.state.output_device_name.is_none(),
                                "Default Output",
                            )
                            .clicked()
                        {
                            self.state.output_device_name = None;
                            self.restart_audio();
                        }
                        for name in &output_devices {
                            if ui
                                .selectable_label(
                                    self.state.output_device_name.as_deref() == Some(name),
                                    name,
                                )
                                .clicked()
                            {
                                self.state.output_device_name = Some(name.clone());
                                self.restart_audio();
                            }
                        }
                    });

                egui::ComboBox::from_label("Sample Rate")
                    .selected_text(format!("{}", self.state.sample_rate_hz))
                    .show_ui(ui, |ui| {
                        for sr in [44_100u32, 48_000u32, 96_000u32] {
                            if ui
                                .selectable_label(self.state.sample_rate_hz == sr, format!("{sr}"))
                                .clicked()
                            {
                                self.state.sample_rate_hz = sr;
                                self.restart_audio();
                            }
                        }
                    });

                egui::ComboBox::from_label("Buffer")
                    .selected_text(format!("{}", self.state.buffer_size))
                    .show_ui(ui, |ui| {
                        for bs in [128u32, 256u32, 512u32] {
                            if ui
                                .selectable_label(self.state.buffer_size == bs, format!("{bs}"))
                                .clicked()
                            {
                                self.state.buffer_size = bs;
                                self.restart_audio();
                            }
                        }
                    });
            });

            if let AudioStatus::Error(msg) = &self.status {
                ui.colored_label(Color32::YELLOW, msg);
            }
        });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(Color32::from_rgb(26, 23, 48))
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(&ctx, |ui| {
                egui::Frame::default()
                    .fill(Color32::from_rgb(39, 33, 69))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Auto-Wah")
                                        .size(22.0)
                                        .strong()
                                        .color(Color32::from_rgb(255, 214, 64)),
                                );
                                ui.add_space(12.0);
                                ui.label(
                                    egui::RichText::new("Guitar In \u{2192} Filter Out")
                                        .size(13.0)
                                        .color(Color32::from_rgb(180, 174, 210)),
                                );
                            });
                            ui.add_space(8.0);

                            // Filter Mode + Direction + Pre-Comp toggles
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Filter Mode:")
                                        .size(13.0)
                                        .color(Color32::from_gray(180)),
                                );
                                let current_mode = self.params.get_filter_mode();
                                for mode in [FilterMode::LowPass, FilterMode::BandPass, FilterMode::HighPass] {
                                    let selected = current_mode == mode;
                                    let btn = egui::Button::new(
                                        egui::RichText::new(mode.label())
                                            .size(14.0)
                                            .color(if selected {
                                                Color32::from_rgb(25, 22, 35)
                                            } else {
                                                Color32::LIGHT_GRAY
                                            }),
                                    );
                                    if ui.add_sized([44.0, 26.0], btn).clicked() && self.can_click_button() {
                                        self.params.set_filter_mode(mode);
                                        self.state.filter_mode = mode as u32;
                                    }
                                }

                                ui.add_space(20.0);

                                ui.label(
                                    egui::RichText::new("Direction:")
                                        .size(13.0)
                                        .color(Color32::from_gray(180)),
                                );
                                let current_dir = self.params.get_direction();
                                for dir in [Direction::Up, Direction::Down] {
                                    let selected = current_dir == dir;
                                    let btn = egui::Button::new(
                                        egui::RichText::new(dir.label())
                                            .size(14.0)
                                            .color(if selected {
                                                Color32::from_rgb(25, 22, 35)
                                            } else {
                                                Color32::LIGHT_GRAY
                                            }),
                                    );
                                    if ui.add_sized([52.0, 26.0], btn).clicked() && self.can_click_button() {
                                        self.params.set_direction(dir);
                                        self.state.direction = dir as u32;
                                    }
                                }

                                ui.add_space(20.0);

                                let pre_comp = self.params.get_pre_comp_on();
                                let toggle_label = if pre_comp { "PRE-COMP: ON" } else { "PRE-COMP: OFF" };
                                let btn = egui::Button::new(
                                    egui::RichText::new(toggle_label)
                                        .size(13.0)
                                        .color(if pre_comp {
                                            Color32::from_rgb(25, 22, 35)
                                        } else {
                                            Color32::LIGHT_GRAY
                                        }),
                                );
                                if ui.add_sized([110.0, 26.0], btn).clicked() && self.can_click_button() {
                                    let new_val = !pre_comp;
                                    self.params.set_pre_comp_on(new_val);
                                    self.state.pre_comp_on = new_val;
                                }
                            });
                            ui.add_space(8.0);

                            // Knobs + Meters
                            ui.horizontal(|ui| {
                                // Left: knobs
                                ui.vertical(|ui| {
                                    // Row 1: Envelope filter knobs
                                    ui.horizontal(|ui| {
                                        let mut sens = self.params.get_sensitivity();
                                        if Self::knob(ui, &mut sens, params::SENSITIVITY_MIN..=params::SENSITIVITY_MAX, "Sensitivity", "", true) {
                                            self.params.set_sensitivity(sens);
                                            self.state.sensitivity = sens;
                                        }

                                        let mut fmin = self.params.get_freq_min();
                                        if Self::knob(ui, &mut fmin, params::FREQ_MIN_MIN..=params::FREQ_MIN_MAX, "Freq Min", " Hz", true) {
                                            self.params.set_freq_min(fmin);
                                            self.state.freq_min_hz = fmin;
                                        }

                                        let mut fmax = self.params.get_freq_max();
                                        if Self::knob(ui, &mut fmax, params::FREQ_MAX_MIN..=params::FREQ_MAX_MAX, "Freq Max", " Hz", true) {
                                            self.params.set_freq_max(fmax);
                                            self.state.freq_max_hz = fmax;
                                        }

                                        let mut q = self.params.get_resonance();
                                        if Self::knob(ui, &mut q, params::RESONANCE_MIN..=params::RESONANCE_MAX, "Resonance", " Q", true) {
                                            self.params.set_resonance(q);
                                            self.state.resonance = q;
                                        }
                                    });

                                    // Row 2: Attack, Release, Mix
                                    ui.horizontal(|ui| {
                                        let mut att = self.params.get_attack_ms();
                                        if Self::knob(ui, &mut att, params::ATTACK_MIN..=params::ATTACK_MAX, "Attack", " ms", true) {
                                            self.params.set_attack_ms(att);
                                            self.state.attack_ms = att;
                                        }

                                        let mut rel = self.params.get_release_ms();
                                        if Self::knob(ui, &mut rel, params::RELEASE_MIN..=params::RELEASE_MAX, "Release", " ms", true) {
                                            self.params.set_release_ms(rel);
                                            self.state.release_ms = rel;
                                        }

                                        let mut mix = self.params.get_dry_wet_percent();
                                        if Self::knob(ui, &mut mix, params::MIX_MIN..=params::MIX_MAX, "Dry/Wet", " %", true) {
                                            self.params.set_dry_wet_percent(mix);
                                            self.state.dry_wet_percent = mix;
                                        }
                                    });

                                    // Row 3: Compressor knobs
                                    ui.horizontal(|ui| {
                                        let comp_on = self.params.get_pre_comp_on();

                                        let mut thresh = self.params.get_comp_threshold();
                                        if Self::knob(ui, &mut thresh, params::COMP_THRESHOLD_MIN..=params::COMP_THRESHOLD_MAX, "Threshold (Comp)", " dB", comp_on) {
                                            self.params.set_comp_threshold(thresh);
                                            self.state.comp_threshold_db = thresh;
                                        }

                                        let mut ratio = self.params.get_comp_ratio();
                                        if Self::knob(ui, &mut ratio, params::COMP_RATIO_MIN..=params::COMP_RATIO_MAX, "Ratio (Comp)", ":1", comp_on) {
                                            self.params.set_comp_ratio(ratio);
                                            self.state.comp_ratio = ratio;
                                        }

                                        let mut makeup = self.params.get_makeup_db();
                                        if Self::knob(ui, &mut makeup, params::MAKEUP_MIN..=params::MAKEUP_MAX, "Makeup", " dB", comp_on) {
                                            self.params.set_makeup_db(makeup);
                                            self.state.makeup_db = makeup;
                                        }
                                    });
                                });

                                ui.add_space(12.0);

                                // Right: meters
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        Self::meter_bar(
                                            ui,
                                            "INPUT",
                                            self.meter_data.input_peak_db,
                                            self.input_peak_hold.value(),
                                            Color32::from_rgb(0x00, 0xAA, 0xFF),
                                        );

                                        Self::envelope_meter(
                                            ui,
                                            "ENV",
                                            self.meter_data.envelope,
                                            Color32::from_rgb(0xFF, 0xAA, 0x00),
                                        );

                                        Self::cutoff_meter(
                                            ui,
                                            "CUTOFF",
                                            self.meter_data.cutoff_hz,
                                            self.params.get_freq_min(),
                                            self.params.get_freq_max(),
                                            Color32::from_rgb(0x00, 0xFF, 0x88),
                                        );

                                        Self::meter_bar(
                                            ui,
                                            "OUTPUT",
                                            self.meter_data.output_peak_db,
                                            self.output_peak_hold.value(),
                                            Color32::from_rgb(0x88, 0xFF, 0x44),
                                        );
                                    });
                                });
                            });
                        });
                    });
            });

        egui::TopBottomPanel::bottom("footer").show(&ctx, |ui| {
            let stats = self.engine.stats.snapshot();
            let cpu = f32::from_bits(stats.cpu_load_bits);
            let latency = f32::from_bits(stats.latency_ms_bits);
            ui.horizontal(|ui| {
                ui.label(format!("CPU: {:>4.1}%", cpu));
                ui.separator();
                ui.label(format!("Latency: {:>4.2} ms", latency));
                ui.separator();
                ui.label(format!("SR: {} Hz", stats.sample_rate_hz));
                ui.separator();
                ui.label(format!("Buffer: {}", stats.buffer_size));
                ui.separator();
                ui.label(egui::RichText::new("Made with Rust + egui + cpal").color(Color32::GRAY));
            });
        });
    }
}

impl Drop for DuckerApp {
    fn drop(&mut self) {
        let _ = self.engine.command_tx.try_send(AudioCommand::Stop);
    }
}
