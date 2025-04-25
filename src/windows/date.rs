use chrono::Local;
use gtk4::glib::{self, ControlFlow};
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Calendar, Label, Orientation, Popover, Separator};
use std::{cell::Cell, cell::RefCell, rc::Rc, time::Duration};

use crate::utils::BarConfig;

const POMODORO_STEP_MINUTES: u32 = 5;
const POMODORO_MIN_MINUTES: u32 = 5;
const POMODORO_MAX_MINUTES: u32 = 120;
const ONE_SECOND: Duration = Duration::from_secs(1);

struct PomodoroState {
    target_minutes: u32,
    remaining_seconds: u32,
    is_running: bool,
    timer_source_id: Option<glib::SourceId>,
}

impl PomodoroState {
    fn new(initial_minutes: u32) -> Self {
        Self {
            target_minutes: initial_minutes,
            remaining_seconds: initial_minutes * 60,
            is_running: false,
            timer_source_id: None,
        }
    }

    fn format_time(&self) -> String {
        let minutes = self.remaining_seconds / 60;
        let seconds = self.remaining_seconds % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }

    fn reset_time(&mut self) {
        self.remaining_seconds = self.target_minutes * 60;
    }
}

pub struct DateWindow {
    popover: Popover,
    #[allow(dead_code)]
    pomodoro_state: Rc<RefCell<PomodoroState>>,
}

impl DateWindow {
    pub fn new(_config: &BarConfig) -> Self {
        let popover = Popover::new();
        popover.add_css_class("DatePopupWindow");

        let main_box = GtkBox::new(Orientation::Vertical, 15);
        main_box.set_margin_top(15);
        main_box.set_margin_bottom(15);
        main_box.set_margin_start(15);
        main_box.set_margin_end(15);
        main_box.set_width_request(350);

        let pomodoro_state = Rc::new(RefCell::new(PomodoroState::new(25)));

        let pomodoro_box = GtkBox::new(Orientation::Horizontal, 10);
        pomodoro_box.add_css_class("pomodoro-controls");

        let decrease_button = Button::from_icon_name("list-remove-symbolic");
        decrease_button.add_css_class("pomodoro-button");
        decrease_button.add_css_class("decrease-button");

        let pomodoro_label = Label::builder()
            .label(&pomodoro_state.borrow().format_time())
            .halign(Align::Center)
            .build();
        pomodoro_label.add_css_class("pomodoro-label");

        let increase_button = Button::from_icon_name("list-add-symbolic");
        increase_button.add_css_class("pomodoro-button");
        increase_button.add_css_class("increase-button");

        let start_pause_button = Button::from_icon_name("media-playback-start-symbolic");
        start_pause_button.add_css_class("pomodoro-button");
        start_pause_button.add_css_class("start-pause-button");

        pomodoro_box.append(&decrease_button);
        pomodoro_box.append(&pomodoro_label);
        pomodoro_box.append(&increase_button);
        pomodoro_box.append(&start_pause_button);

        let live_clock_label = Label::builder()
            .label(&Local::now().format("%H:%M:%S").to_string())
            .halign(Align::End)
            .hexpand(true)
            .build();
        live_clock_label.add_css_class("live-clock-label");

        let top_row_box = GtkBox::new(Orientation::Horizontal, 20);
        top_row_box.append(&pomodoro_box);
        top_row_box.append(&live_clock_label);

        main_box.append(&top_row_box);
        main_box.append(&Separator::new(Orientation::Horizontal));

        let calendar = Calendar::new();
        calendar.add_css_class("date-calendar");
        main_box.append(&calendar);

        popover.set_child(Some(&main_box));

        let clock_label_weak = live_clock_label.downgrade();
        let timer_source_id = Rc::new(Cell::new(None::<glib::SourceId>));

        let timer_source_id_clone = timer_source_id.clone();
        let source_id = glib::timeout_add_local(ONE_SECOND, move || {
            if let Some(label) = clock_label_weak.upgrade() {
                label.set_label(&Local::now().format("%H:%M:%S").to_string());
                ControlFlow::Continue
            } else {
                timer_source_id_clone.set(None);
                ControlFlow::Break
            }
        });
        timer_source_id.set(Some(source_id));

        let timer_source_id_destroy = timer_source_id.clone();
        popover.connect_destroy(move |_| {
            if let Some(id) = timer_source_id_destroy.take() {
                id.remove();
            }
        });

        let state_clone_decrease = pomodoro_state.clone();
        let label_clone_decrease = pomodoro_label.clone();
        decrease_button.connect_clicked(move |_| {
            let mut state = state_clone_decrease.borrow_mut();
            if !state.is_running && state.target_minutes > POMODORO_MIN_MINUTES {
                state.target_minutes =
                    (state.target_minutes - POMODORO_STEP_MINUTES).max(POMODORO_MIN_MINUTES);
                state.reset_time();
                label_clone_decrease.set_label(&state.format_time());
            }
        });

        let state_clone_increase = pomodoro_state.clone();
        let label_clone_increase = pomodoro_label.clone();
        increase_button.connect_clicked(move |_| {
            let mut state = state_clone_increase.borrow_mut();
            if !state.is_running && state.target_minutes < POMODORO_MAX_MINUTES {
                state.target_minutes =
                    (state.target_minutes + POMODORO_STEP_MINUTES).min(POMODORO_MAX_MINUTES);
                state.reset_time();
                label_clone_increase.set_label(&state.format_time());
            }
        });

        let state_clone_toggle = pomodoro_state.clone();
        let label_clone_toggle = pomodoro_label.clone();
        let start_pause_clone = start_pause_button.clone();
        let decrease_clone = decrease_button.clone();
        let increase_clone = increase_button.clone();
        let popover_weak = popover.downgrade();
        start_pause_button.connect_clicked(move |btn| {
            let Some(_popover) = popover_weak.upgrade() else {
                return;
            };
            let mut state = state_clone_toggle.borrow_mut();
            if state.is_running {
                state.is_running = false;
                if let Some(id) = state.timer_source_id.take() {
                    id.remove();
                }
                btn.set_icon_name("media-playback-start-symbolic");
                decrease_clone.set_sensitive(true);
                increase_clone.set_sensitive(true);
            } else {
                if state.remaining_seconds == 0 {
                    state.reset_time();
                    label_clone_toggle.set_label(&state.format_time());
                }
                state.is_running = true;
                btn.set_icon_name("media-playback-pause-symbolic");
                decrease_clone.set_sensitive(false);
                increase_clone.set_sensitive(false);

                let state_rc = state_clone_toggle.clone();
                let label_rc = label_clone_toggle.clone();
                let button_rc = start_pause_clone.clone();
                let decrease_rc = decrease_clone.clone();
                let increase_rc = increase_clone.clone();
                let popover_rc_weak = popover_weak.clone();

                let timer_id = glib::timeout_add_local(ONE_SECOND, move || {
                    let mut state = state_rc.borrow_mut();
                    if !state.is_running {
                        return ControlFlow::Break;
                    }
                    if state.remaining_seconds > 0 {
                        state.remaining_seconds -= 1;
                        label_rc.set_label(&state.format_time());
                        ControlFlow::Continue
                    } else {
                        state.is_running = false;
                        state.timer_source_id = None;
                        button_rc.set_icon_name("media-playback-start-symbolic");
                        decrease_rc.set_sensitive(true);
                        increase_rc.set_sensitive(true);

                        if let Some(p) = popover_rc_weak.upgrade() {
                            p.popup();
                        }

                        ControlFlow::Break
                    }
                });
                state.timer_source_id = Some(timer_id);
            }
        });

        let state_clone_close = pomodoro_state.clone();
        let start_pause_clone_close = start_pause_button.clone();
        let decrease_clone_close = decrease_button.clone();
        let increase_clone_close = increase_button.clone();
        popover.connect_closed(move |_popover| {
            let mut state = state_clone_close.borrow_mut();
            if state.is_running {
                state.is_running = false;
                if let Some(id) = state.timer_source_id.take() {
                    id.remove();
                }
                start_pause_clone_close.set_icon_name("media-playback-start-symbolic");
                decrease_clone_close.set_sensitive(true);
                increase_clone_close.set_sensitive(true);
            }
        });

        Self {
            popover,
            pomodoro_state: pomodoro_state.clone(),
        }
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }
}
