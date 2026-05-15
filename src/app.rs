use crate::{
    audio::{
        list_input_devices, list_output_devices, spawn_audio_engine, AudioCommand,
        AudioEngineHandle, AudioSettings, AudioStatus,
    },
    meter::{MeterData, PeakHold},
    params::Params,
};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde::{Deserialize, Serialize};
use std::{f32::consts::TAU, sync::Arc};

const APP_KEY: &str = "ducker-app-state";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedState {
    quack_intensity: f32,
    input_device_name: Option<String>,
    output_device_name: Option<String>,
    main_channel: usize,
    sidechain_channel: usize,
    sample_rate_hz: u32,
    buffer_size: u32,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            quack_intensity: 1.0,
            input_device_name: None,
            output_device_name: None,
            main_channel: 0,
            sidechain_channel: 1,
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
    side_peak_hold: PeakHold,
    gr_peak_hold: PeakHold,
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
        params.set_quack_intensity(state.quack_intensity);

        let engine = spawn_audio_engine(
            Arc::clone(&params),
            AudioSettings {
                input_device_name: state.input_device_name.clone(),
                output_device_name: state.output_device_name.clone(),
                main_channel: state.main_channel,
                sidechain_channel: state.sidechain_channel,
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
            side_peak_hold: PeakHold::new(),
            gr_peak_hold: PeakHold::new(),
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
                sidechain_channel: self.state.sidechain_channel,
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
    ) -> bool {
        let desired = Vec2::new(84.0, 112.0);
        let (rect, mut response) = ui.allocate_exact_size(desired, Sense::drag());

        if response.dragged() {
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
        let accent = Color32::from_rgb(0x00, 0xAA, 0xFF);
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
        painter.circle_filled(dot, 3.5, Color32::WHITE);

        painter.text(
            Pos2::new(rect.center().x, rect.top() + 4.0),
            egui::Align2::CENTER_TOP,
            format!("{value:.1}{suffix}"),
            egui::FontId::proportional(13.0),
            Color32::LIGHT_GRAY,
        );
        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 8.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(12.0),
            Color32::from_gray(180),
        );

        response.changed()
    }

    fn meter_bar(
        ui: &mut egui::Ui,
        label: &str,
        db: f32,
        peak_db: f32,
        inverted: bool,
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

        let norm = if inverted {
            (db.abs() / 30.0).clamp(0.0, 1.0)
        } else {
            ((db + 60.0) / 60.0).clamp(0.0, 1.0)
        };

        let fill_top = bottom - (height * norm);
        let fill_rect = Rect::from_min_max(
            Pos2::new(rect.left() + 8.0, fill_top),
            Pos2::new(rect.right() - 8.0, bottom),
        );
        painter.rect_filled(fill_rect, 2.0, accent);

        let hold_norm = if inverted {
            (peak_db.abs() / 30.0).clamp(0.0, 1.0)
        } else {
            ((peak_db + 60.0) / 60.0).clamp(0.0, 1.0)
        };
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

    fn draw_duck_mascot(ui: &mut egui::Ui, time_s: f32, activity: f32) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(180.0, 130.0), Sense::hover());
        let painter = ui.painter_at(rect);

        let activity = activity.clamp(0.0, 1.0);
        let bob = (time_s * 3.2).sin() * (2.0 + (activity * 5.0));
        let wing_swing = (time_s * (7.0 + activity * 5.0)).sin() * (3.0 + activity * 7.0);

        let body = Pos2::new(rect.center().x, rect.center().y + 22.0 + bob);
        let head = Pos2::new(rect.center().x + 30.0, rect.center().y - 4.0 + bob);

        let yellow = Color32::from_rgb(255, 220, 55);
        let yellow_dark = Color32::from_rgb(230, 190, 35);
        let orange = Color32::from_rgb(255, 140, 35);

        painter.circle_filled(body, 42.0, yellow);
        painter.circle_filled(head, 28.0, yellow);
        painter.circle_stroke(body, 42.0, Stroke::new(2.0, yellow_dark));
        painter.circle_stroke(head, 28.0, Stroke::new(2.0, yellow_dark));

        let wing = [
            Pos2::new(body.x - 4.0, body.y - 10.0),
            Pos2::new(body.x - 24.0 - wing_swing, body.y + 8.0 - wing_swing * 0.25),
            Pos2::new(body.x + 8.0, body.y + 18.0),
        ];
        painter.add(egui::Shape::convex_polygon(
            wing.to_vec(),
            yellow_dark,
            Stroke::NONE,
        ));

        let beak = [
            Pos2::new(head.x + 18.0, head.y + 3.0),
            Pos2::new(head.x + 48.0, head.y + 10.0),
            Pos2::new(head.x + 18.0, head.y + 16.0),
        ];
        painter.add(egui::Shape::convex_polygon(
            beak.to_vec(),
            orange,
            Stroke::new(1.0, Color32::from_rgb(220, 110, 30)),
        ));

        let eye = Pos2::new(head.x + 7.0, head.y - 5.0);
        painter.circle_filled(eye, 5.0, Color32::BLACK);
        painter.circle_filled(Pos2::new(eye.x - 1.5, eye.y - 1.5), 1.6, Color32::WHITE);

        let bubble_alpha = (40.0 + activity * 160.0) as u8;
        painter.text(
            Pos2::new(head.x + 38.0, head.y - 28.0),
            egui::Align2::CENTER_CENTER,
            "quack!",
            egui::FontId::proportional(14.0),
            Color32::from_rgba_unmultiplied(255, 242, 120, bubble_alpha),
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
        self.side_peak_hold
            .update(self.meter_data.sidechain_peak_db);
        self.gr_peak_hold
            .update(self.meter_data.gain_reduction_db.abs());
        self.output_peak_hold.update(self.meter_data.output_peak_db);

        egui::TopBottomPanel::top("title").show(&ctx, |ui| {
            ui.set_height(92.0);
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("DUCKER  🦆")
                        .monospace()
                        .size(34.0)
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

                egui::ComboBox::from_label("SC Ch")
                    .selected_text(format!("{}", self.state.sidechain_channel + 1))
                    .show_ui(ui, |ui| {
                        for ch in 0..16usize {
                            if ui
                                .selectable_label(
                                    self.state.sidechain_channel == ch,
                                    format!("{}", ch + 1),
                                )
                                .clicked()
                            {
                                self.state.sidechain_channel = ch;
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
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(&ctx, |ui| {
                egui::Frame::default()
                    .fill(Color32::from_rgb(39, 33, 69))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(20))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            let time_s = ctx.input(|i| i.time) as f32;
                            let activity = ((self.meter_data.input_peak_db + 60.0) / 60.0).clamp(0.0, 1.0);

                            ui.label(
                                egui::RichText::new("Make It Quack")
                                    .size(24.0)
                                    .strong()
                                    .color(Color32::from_rgb(255, 214, 64)),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Guitar In -> Duck Out")
                                    .size(14.0)
                                    .color(Color32::from_rgb(180, 174, 210)),
                            );
                            ui.add_space(10.0);

                            Self::draw_duck_mascot(ui, time_s, activity);
                            ui.add_space(8.0);

                            ui.horizontal(|ui| {
                                if ui
                                    .add_sized([180.0, 44.0], egui::Button::new("Less Quack"))
                                    .clicked()
                                    && self.can_click_button()
                                {
                                    let mut intensity = self.params.get_quack_intensity();
                                    intensity = (intensity - 0.2).max(0.1);
                                    self.params.set_quack_intensity(intensity);
                                    self.state.quack_intensity = intensity;
                                }

                                ui.add_space(12.0);

                                if ui
                                    .add_sized([180.0, 44.0], egui::Button::new("More Quack"))
                                    .clicked()
                                    && self.can_click_button()
                                {
                                    let mut intensity = self.params.get_quack_intensity();
                                    intensity = (intensity + 0.2).min(10.0);
                                    self.params.set_quack_intensity(intensity);
                                    self.state.quack_intensity = intensity;
                                }
                            });

                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Quack Intensity: {:.1}x",
                                    self.params.get_quack_intensity()
                                ))
                                .size(18.0)
                                .color(Color32::from_rgb(255, 235, 125)),
                            );
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
