//! The reminder to chip in.
//!
//! Modelled on the one everybody already knows and nobody actually hates: it
//! appears when the program starts, it goes away with one click, and it has
//! never stopped anybody doing anything. There is deliberately **no "never
//! show this again"** — that button is what kills these: it gets pressed on the
//! first day and then there are no reminders for ever. Insisting quietly and
//! harmlessly for years is what works.
//!
//! With one rule the original never needed: **not during a show.** If timecode
//! is already arriving when the program starts, or the show is armed, this
//! run says nothing at all. A dialog over the window ten minutes before doors
//! is a serious fault wearing marketing's clothes, and it would only have to
//! happen once for somebody to write about it.

use eframe::egui;

/// How long after starting to show it, so the main window is up and settled
/// first and it is obvious what the reminder belongs to.
const DELAY: f32 = 1.5;

pub struct Reminder {
    /// Seconds left before showing, or `None` once it has had its turn.
    pending: Option<f32>,
    showing: bool,
    /// How many times it has been closed, all told.
    dismissed: u32,
}

impl Reminder {
    pub fn new(dismissed: u32) -> Self {
        Self {
            pending: Some(DELAY),
            showing: false,
            dismissed,
        }
    }

    pub fn dismissed(&self) -> u32 {
        self.dismissed
    }

    /// Tick it along. `busy` means a show is running: armed, or timecode
    /// arriving. While that is true the reminder waits, and if the program is
    /// closed first it simply never appears this run.
    pub fn update(&mut self, delta: f32, busy: bool) {
        if busy {
            // Not postponed — cancelled. Somebody who starts this in a venue
            // and gets straight to work should not be interrupted an hour
            // later either.
            self.pending = None;
            self.showing = false;
            return;
        }
        if let Some(remaining) = &mut self.pending {
            *remaining -= delta;
            if *remaining <= 0.0 {
                self.pending = None;
                self.showing = true;
            }
        }
    }

    #[cfg(test)]
    fn is_showing(&self) -> bool {
        self.showing
    }

    /// Draw it. Returns true when it was closed, so the count can be saved.
    pub fn show(&mut self, context: &egui::Context, paypal: &str) -> bool {
        if !self.showing {
            return false;
        }

        let mut closed = false;
        let viewport = egui::ViewportId::from_hash_of("chasefire-reminder");
        let builder = egui::ViewportBuilder::default()
            .with_title("Chasefire")
            .with_inner_size([420.0, 250.0])
            .with_resizable(false);

        context.show_viewport_immediate(viewport, builder, |context, _class| {
            egui::CentralPanel::default().show(context, |ui| {
                ui.add_space(14.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Chasefire is free, and stays free")
                            .size(17.0)
                            .strong(),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "No licence, no expiry, nothing switched off if you never pay.\n\
                             If it earns you money, put something in once — what you think\n\
                             it is worth. Never a subscription.",
                        )
                        .size(12.5),
                    );

                    ui.add_space(14.0);
                    let donate = egui::Button::new(
                        egui::RichText::new("Donate")
                            .size(15.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(0, 112, 186))
                    .min_size(egui::vec2(150.0, 34.0));
                    if ui.add(donate).on_hover_text(paypal).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(paypal));
                    }

                    ui.add_space(8.0);
                    if ui
                        .add(egui::Button::new("Not today").min_size(egui::vec2(150.0, 28.0)))
                        .clicked()
                    {
                        closed = true;
                    }

                    // Honest, and funnier than a number nobody can see.
                    if self.dismissed > 0 {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "You have closed this {} times.",
                                self.dismissed
                            ))
                            .size(11.0)
                            .weak(),
                        );
                    }
                });
            });
            if context.input(|input| input.viewport().close_requested()) {
                closed = true;
            }
        });

        if closed {
            self.showing = false;
            self.dismissed = self.dismissed.saturating_add(1);
        }
        closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_waits_a_moment_before_appearing() {
        let mut reminder = Reminder::new(0);
        reminder.update(DELAY / 2.0, false);
        assert!(!reminder.is_showing(), "appeared before the window settled");
        reminder.update(DELAY, false);
        assert!(reminder.is_showing());
    }

    #[test]
    fn a_running_show_cancels_it_for_good() {
        // The rule that matters. Not postponed — cancelled, so it cannot turn
        // up an hour into the evening either.
        let mut reminder = Reminder::new(3);
        reminder.update(0.1, true);
        assert!(!reminder.is_showing());
        for _ in 0..100 {
            reminder.update(1.0, false);
        }
        assert!(!reminder.is_showing(), "came back after a show had started");
    }

    #[test]
    fn it_never_appears_twice_in_one_run() {
        let mut reminder = Reminder::new(0);
        reminder.update(DELAY + 0.1, false);
        assert!(reminder.is_showing());
        reminder.showing = false; // as if closed
        for _ in 0..100 {
            reminder.update(1.0, false);
        }
        assert!(!reminder.is_showing(), "came back on its own");
    }
}
