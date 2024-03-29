use std::collections::HashMap;

use egui::{
    collapsing_header::CollapsingState, Button, CentralPanel, Color32, Frame, Label, Layout,
    RichText, Rounding, ScrollArea, Sense, TextEdit,
};
use poll_promise::Promise;
use sequence_generator::sequence_generator;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct WRedNetDbgApp {
    base_url: String,
    secret: String,
    #[serde(skip)]
    pub log_cache: HashMap<u64, Promise<Result<wred_server::LogEntry, String>>>,
    #[serde(skip)]
    pub log_cache_ents: Option<Promise<Result<Vec<wred_server::LogEntryPartial>, String>>>,
}

impl Default for WRedNetDbgApp {
    fn default() -> Self {
        #[cfg(target_arch = "wasm32")]
        let base_url = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .location()
            .unwrap()
            .origin()
            .unwrap();
        #[cfg(not(target_arch = "wasm32"))]
        let base_url = "http://localhost:8080".to_string();
        Self {
            base_url,
            secret: String::new(),
            log_cache: HashMap::default(),
            log_cache_ents: None,
        }
    }
}

impl WRedNetDbgApp {
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_fonts(crate::style::get_fonts());

        let mut style = (*cc.egui_ctx.style()).clone();
        crate::style::fix_style(&mut style);
        cc.egui_ctx.set_style(style);

        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Self::default()
    }
}

impl eframe::App for WRedNetDbgApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel")
            .frame(
                Frame::menu(&ctx.style())
                    .inner_margin(6.0)
                    .rounding(if cfg!(not(target_os = "macos")) {
                        Rounding {
                            sw: 6.0,
                            se: 6.0,
                            ..Default::default()
                        }
                    } else {
                        Rounding::none()
                    })
                    .fill(Color32::from_rgba_premultiplied(0x20, 0x20, 0x20, 0xFF)),
            )
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    ui.add_space(62.5);
                    ui.style_mut().spacing.item_spacing.x = 5.0;
                    ui.heading("WhateverRed");
                    ui.separator();
                    ui.label(RichText::new("NETDBG").small().monospace());
                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(Button::new(RichText::new("\u{1F504}").heading()).frame(false))
                            .clicked()
                        {
                            self.log_cache.clear();
                            self.log_cache_ents = None;
                        }
                        ui.separator();
                        ui.add(
                            TextEdit::singleline(&mut self.secret)
                                .password(true)
                                .hint_text("Admin Secret"),
                        );
                        ui.add(TextEdit::singleline(&mut self.base_url).hint_text("Base URL"));
                    });
                });
            });

        let cached_promise = self.log_cache_ents.get_or_insert_with(|| {
            let ctx = ctx.clone();
            let (sender, promise) = Promise::new();
            let request = ehttp::Request::get(format!("{}/all", self.base_url));
            ehttp::fetch(request, move |response| {
                let ent = response
                    .and_then(|v| postcard::from_bytes(&v.bytes).map_err(|e| e.to_string()));
                sender.send(ent);
                ctx.request_repaint();
            });
            promise
        });

        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| match cached_promise.ready() {
                None => {
                    ui.spinner();
                }
                Some(Err(e)) => {
                    ui.colored_label(Color32::RED, RichText::new(e).heading().strong());
                }
                Some(Ok(ents)) => {
                    ui.set_width(ui.available_width());

                    for ent in ents {
                        let cached_promise = self.log_cache.entry(ent.id).or_insert_with(|| {
                            let ctx = ctx.clone();
                            let (sender, promise) = Promise::new();
                            let request =
                                ehttp::Request::get(format!("{}/{}", self.base_url, ent.id));
                            ehttp::fetch(request, move |response| {
                                let ent = response.and_then(|v| {
                                    postcard::from_bytes(&v.bytes).map_err(|e| e.to_string())
                                });
                                sender.send(ent);
                                ctx.request_repaint();
                            });
                            promise
                        });
                        Frame::group(&ctx.style())
                            .fill(Color32::from_rgba_premultiplied(0x20, 0x20, 0x20, 0xFF))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                CollapsingState::load_with_default_open(
                                    ctx,
                                    ui.make_persistent_id(ent.id),
                                    false,
                                )
                                .show_header(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            Label::new(
                                                RichText::new(ent.addr.to_string()).strong(),
                                            )
                                            .sense(Sense::click()),
                                        )
                                        .context_menu(
                                            |ui| {
                                                if ui.button("\u{1F5D0} Copy IP").clicked() {
                                                    ui.output().copied_text = ent.addr.to_string();
                                                    ui.close_menu();
                                                }
                                            },
                                        );
                                        let props = wred_server::get_id_props();
                                        let ms = sequence_generator::decode_id_unix_epoch_micros(
                                            ent.id, &props,
                                        );
                                        let d = std::time::UNIX_EPOCH
                                            + std::time::Duration::from_micros(ms);
                                        let localtime = chrono::DateTime::<chrono::Local>::from(d);
                                        let fmter =
                                            timeago::Formatter::with_language(timeago::English);
                                        let now = chrono::Local::now();
                                        ui.label(
                                            RichText::new(fmter.convert_chrono(localtime, now))
                                                .weak(),
                                        );
                                        let d = std::time::UNIX_EPOCH
                                            + std::time::Duration::from_micros(ent.last_updated);
                                        let localtime = chrono::DateTime::<chrono::Local>::from(d);
                                        ui.separator();
                                        ui.label(RichText::new("last updated").weak());
                                        ui.label(
                                            RichText::new(fmter.convert_chrono(localtime, now))
                                                .weak(),
                                        );
                                    });
                                    ui.with_layout(
                                        Layout::right_to_left(egui::Align::Center),
                                        |ui| match cached_promise.ready() {
                                            None => {
                                                ui.spinner();
                                            }
                                            Some(Err(_)) => {
                                                ui.add(
                                                    Button::new(
                                                        RichText::new("\u{1F5D9}").heading(),
                                                    )
                                                    .frame(false),
                                                );
                                            }
                                            Some(Ok(ent_full)) => {
                                                if ui
                                                    .add(
                                                        Button::new(
                                                            RichText::new("\u{2B8B}").heading(),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .on_hover_text("Save to file")
                                                    .clicked()
                                                {
                                                    ui.output().copied_text = ent_full.data.clone();
                                                }
                                                if ui
                                                    .add(
                                                        Button::new(
                                                            RichText::new("\u{1F5D0}").heading(),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .on_hover_text("\u{1F5D0} Copy text")
                                                    .clicked()
                                                {
                                                    ui.output().copied_text = ent_full.data.clone();
                                                }
                                                let resp = ui
                                                    .add_enabled(
                                                        !self.secret.is_empty(),
                                                        Button::new(
                                                            RichText::new("\u{274C}").heading(),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .on_hover_text("Discard/Delete");
                                                let id = resp.id.with("discard_confirmation");
                                                egui::popup::popup_below_widget(
                                                    ui,
                                                    id,
                                                    &resp,
                                                    |ui| {
                                                        ui.set_min_width(100.0);
                                                        ui.label("Are you sure?");
                                                        ui.horizontal(|ui| {
                                                            if ui.button("Yes").clicked() {
                                                                ui.memory().close_popup();
                                                                let ctx = ctx.clone();
                                                                let request = ehttp::Request {
                                                                    method: "DELETE".to_owned(),
                                                                    url: format!(
                                                                        "{}/{}",
                                                                        self.base_url, ent.id
                                                                    ),
                                                                    body: postcard::to_allocvec(
                                                                        &self.secret,
                                                                    )
                                                                    .unwrap(),
                                                                    ..ehttp::Request::get("")
                                                                };
                                                                ehttp::fetch(
                                                                    request,
                                                                    move |response| {
                                                                        if let Err(e) = response {
                                                                            eprintln!(
                                                                                "Error: {}",
                                                                                e
                                                                            );
                                                                        }
                                                                        ctx.request_repaint();
                                                                    },
                                                                );
                                                            }
                                                            if ui.button("No").clicked() {
                                                                ui.memory().close_popup();
                                                            }
                                                        });
                                                    },
                                                );
                                                if resp.clicked() {
                                                    ui.memory().open_popup(id);
                                                }
                                                let resp = ui
                                                    .add_enabled(
                                                        !self.secret.is_empty(),
                                                        Button::new(
                                                            RichText::new("\u{2705}").heading(),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .on_hover_text("Save to server");
                                                let id = resp.id.with("save_confirmation");

                                                let save = || {
                                                    let ctx = ctx.clone();
                                                    let request = ehttp::Request::post(
                                                        format!("{}/{}", self.base_url, ent.id),
                                                        postcard::to_allocvec(&self.secret)
                                                            .unwrap(),
                                                    );
                                                    ehttp::fetch(request, move |response| {
                                                        if let Err(e) = response {
                                                            eprintln!("Error: {}", e);
                                                        }
                                                        ctx.request_repaint();
                                                    });
                                                };
                                                egui::popup::popup_below_widget(
                                                    ui,
                                                    id,
                                                    &resp,
                                                    |ui| {
                                                        ui.set_min_width(100.0);
                                                        ui.label("Are you sure?");
                                                        ui.horizontal(|ui| {
                                                            if ui.button("Yes").clicked() {
                                                                ui.memory().close_popup();
                                                                save();
                                                            }
                                                            if ui.button("No").clicked() {
                                                                ui.memory().close_popup();
                                                            }
                                                        });
                                                    },
                                                );
                                                if resp.clicked() {
                                                    if ent.is_saved {
                                                        ui.memory().open_popup(id);
                                                    } else {
                                                        save();
                                                    }
                                                }
                                            }
                                        },
                                    )
                                })
                                .body(|ui| {
                                    ui.set_width(ui.available_width());

                                    match cached_promise.ready() {
                                        None => {
                                            ui.spinner();
                                        }
                                        Some(Err(e)) => {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new("\u{1F5D9}").heading().strong(),
                                                );
                                                ui.label(e);
                                            });
                                        }
                                        Some(Ok(ent)) => {
                                            ui.label(ent.data.trim());
                                        }
                                    }
                                })
                                .0
                                .context_menu(|ui| {
                                    if ui.button("\u{1F5D0} Copy ID").clicked() {
                                        ui.output().copied_text = ent.id.to_string();
                                        ui.close_menu();
                                    }
                                });
                            });
                    }
                }
            });
        });
    }
}
