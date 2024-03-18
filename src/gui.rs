use nih_plug::prelude::*;
use nih_plug_egui::egui;
use std::sync::{Arc, RwLock};

use crate::{faust::widgets::*, DspType};

pub fn enum_combobox<T: strum::IntoEnumIterator + PartialEq + std::fmt::Debug>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    selected: &mut T,
) {
    egui::ComboBox::from_id_source(id)
        .selected_text(format!("{:?}", selected))
        .show_ui(ui, |ui| {
            for variant in T::iter() {
                let s = format!("{:?}", variant);
                ui.selectable_value(selected, variant, s);
            }
        });
}

pub fn top_panel_contents(
    async_executor: &AsyncExecutor<crate::NihFaustJit>,
    egui_ctx: &egui::Context,
    ui: &mut egui::Ui,
    dsp_nvoices_arc: &Arc<RwLock<i32>>,
    selected_paths_arc: &Arc<RwLock<crate::SelectedPaths>>,
    dsp_lib_path_dialog: &mut Option<egui_file::FileDialog>,
    dsp_script_dialog: &mut Option<egui_file::FileDialog>,
) {
    // Setting the DSP type and number of voices (if applicable):

    let mut nvoices = *dsp_nvoices_arc.read().unwrap();
    let mut selected_dsp_type = DspType::from_nvoices(nvoices);
    let last_dsp_type = selected_dsp_type;
    ui.horizontal(|ui| {
        ui.label("DSP type:");
        enum_combobox(ui, "dsp-type-combobox", &mut selected_dsp_type);
        match selected_dsp_type {
            DspType::AutoDetect => {
                nvoices = -1;
                ui.label("DSP type and number of voices will be detected from script metadata");
            }
            DspType::Effect => {
                nvoices = 0;
                ui.label("DSP will be loaded as monophonic effect");
            }
            DspType::Instrument => {
                if selected_dsp_type != last_dsp_type {
                    // If we just changed dsp_type to
                    // Instrument, we need to set a default
                    // voice number:
                    nvoices = 1;
                }
                ui.add(egui::Slider::new(&mut nvoices, 1..=32).text("voices"));
            }
        }
    });
    *dsp_nvoices_arc.write().unwrap() = nvoices;

    let mut selected_paths = selected_paths_arc.write().unwrap();

    // Setting the Faust libraries path:

    ui.label(format!(
        "Faust DSP libraries path: {}",
        selected_paths.dsp_lib_path.display()
    ));
    if (ui.button("Set Faust libraries path")).clicked() {
        let mut dialog =
            egui_file::FileDialog::select_folder(Some(selected_paths.dsp_lib_path.clone()));
        dialog.open();
        *dsp_lib_path_dialog = Some(dialog);
    }
    if let Some(dialog) = dsp_lib_path_dialog {
        if dialog.show(egui_ctx).selected() {
            if let Some(file) = dialog.path() {
                selected_paths.dsp_lib_path = file.to_path_buf();
            }
        }
    }

    // Setting the DSP script and triggering a reload:

    match &selected_paths.dsp_script {
        Some(path) => ui.label(format!("DSP script: {}", path.display())),
        None => ui.colored_label(egui::Color32::YELLOW, "No DSP script selected"),
    };
    if (ui.add(egui::Button::new("Set or reload DSP script").fill(egui::Color32::DARK_GRAY)))
        .clicked()
    {
        let presel = selected_paths
            .dsp_script
            .as_ref()
            .unwrap_or(&selected_paths.dsp_lib_path);
        let mut dialog = egui_file::FileDialog::open_file(Some(presel.clone()));
        dialog.open();
        *dsp_script_dialog = Some(dialog);
    }
    if let Some(dialog) = dsp_script_dialog {
        if dialog.show(egui_ctx).selected() {
            if let Some(file) = dialog.path() {
                selected_paths.dsp_script = Some(file.to_path_buf());
                async_executor.execute_background(crate::Tasks::ReloadDsp);
            }
        }
    }
}

fn hgroup_header_icon(ui: &mut egui::Ui, openness: f32, response: &egui::Response) {
    let stroke = ui.style().interact(&response).fg_stroke;
    let radius = 3.0;
    if openness == 0.0 {
        ui.painter()
            .circle_filled(response.rect.center(), radius, stroke.color);
    } else {
        ui.painter()
            .circle_stroke(response.rect.center(), radius, stroke);
    }
}

pub fn central_panel_contents(ui: &mut egui::Ui, widgets: &mut [DspWidget<'_>], in_a_tab: bool) {
    for w in widgets {
        match w {
            DspWidget::TabGroup {
                label,
                inner,
                selected,
            } => {
                let id = ui.make_persistent_id(&label);
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    true,
                )
                .show_header(ui, |ui| {
                    ui.label(&*label);
                    for (idx, w) in inner.iter().enumerate() {
                        let mut btn = egui::Button::new(w.label());
                        if *selected == idx {
                            btn = btn.fill(egui::Color32::DARK_BLUE);
                        }
                        if ui.add(btn).clicked() {
                            *selected = idx;
                        }
                    }
                })
                .body(|ui| {
                    central_panel_contents(ui, &mut inner[*selected..=*selected], true);
                });
            }
            DspWidget::Box {
                layout,
                label,
                inner,
            } => {
                let egui_layout = match layout {
                    BoxLayout::Horizontal => egui::Layout::left_to_right(egui::Align::Min),
                    BoxLayout::Vertical => egui::Layout::top_down(egui::Align::Min),
                };
                let mut draw_inner = |ui: &mut egui::Ui| {
                    ui.with_layout(egui_layout, |ui| central_panel_contents(ui, inner, false))
                };
                if in_a_tab || label.is_empty() {
                    draw_inner(ui);
                } else {
                    let mut header = egui::CollapsingHeader::new(&*label).default_open(true);
                    if *layout == BoxLayout::Horizontal {
                        header = header.icon(hgroup_header_icon);
                    }
                    header.show(ui, draw_inner);
                }
            }
            DspWidget::Button {
                layout,
                label,
                zone,
            } => match layout {
                ButtonLayout::GateButton => {
                    let mut btn = egui::Button::new(&*label).sense(egui::Sense::drag());
                    if **zone != 0.0 {
                        // If was held last frame:
                        btn = btn.fill(egui::Color32::YELLOW);
                    }
                    if ui.add(btn).dragged() {
                        // If the button is currently held:
                        **zone = 1.0;
                    } else {
                        **zone = 0.0;
                    }
                }
                ButtonLayout::Checkbox => {
                    let mut selected = **zone != 0.0;
                    ui.checkbox(&mut selected, &*label);
                    **zone = selected as i32 as f32;
                }
            },
            DspWidget::Numeric {
                layout,
                label,
                zone,
                min,
                max,
                step,
                init,
            } => {
                let rng = std::ops::RangeInclusive::new(*min, *max);
                ui.vertical(|ui| {
                    if !label.is_empty() {
                        if ui
                            .label(&*label)
                            .interact(egui::Sense::click())
                            .double_clicked()
                        {
                            **zone = *init;
                        }
                    }
                    match layout {
                        NumericLayout::NumEntry => {
                            ui.add(egui::DragValue::new(*zone).clamp_range(rng))
                        }
                        NumericLayout::HorizontalSlider => {
                            ui.add(egui::Slider::new(*zone, rng).step_by(*step as f64))
                        }
                        NumericLayout::VerticalSlider => ui.add(
                            egui::Slider::new(*zone, rng)
                                .step_by(*step as f64)
                                .vertical(),
                        ),
                    };
                });
            }
            DspWidget::Bargraph {
                layout,
                label,
                zone,
                min,
                max,
            } => {
                // PLACEHOLDER:
                match layout {
                    BargraphLayout::Horizontal => ui.vertical(|ui| {
                        if !label.is_empty() {
                            ui.label(format!("{}:", label));
                        }
                        ui.horizontal(|ui| {
                            ui.label(format!("{:.2}[", min));
                            ui.colored_label(egui::Color32::YELLOW, format!("{:.2}", **zone));
                            ui.label(format!("]{:.2}", max));
                        });
                    }),
                    BargraphLayout::Vertical => ui.vertical(|ui| {
                        if !label.is_empty() {
                            ui.label(format!("{}:", label));
                        }
                        ui.label(format!("_{:.2}_", max));
                        ui.colored_label(egui::Color32::YELLOW, format!(" {:.2}", **zone));
                        ui.label(format!("¨{:.2}¨", min));
                    }),
                };
            }
        }
    }
}
