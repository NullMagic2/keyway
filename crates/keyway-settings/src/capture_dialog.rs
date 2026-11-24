//! Module: keyway_settings::capture_dialog
//! Purpose: The press-to-set dialog for a shortcut.
//! Created: 2026-11-08
//! Architecture: Forwards real key events to shortcut_capture.rs and reports the result;
//! the caller decides whether that shortcut can be used.

use crate::i18n::{Language, Message, text};
use crate::shortcut_capture::{CaptureStep, ShortcutCapture, key_code_from_hardware};
use crate::widgets::{self, DIM, SMALL};
use gtk::glib;
use gtk::prelude::*;
use keyway_core::hotkey::{Chord, ChordError};
use keyway_core::key_codes::{KeyAction, KeyEvent};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const DIALOG_WIDTH: i32 = 380;
const CONTENT_SPACING: i32 = 8;
const CONTENT_MARGIN: i32 = 24;

/// What the person chose in the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureResult {
    Set(Chord),
    Clear,
}

/// A live capture dialog. It stays open after a choice the caller refuses, so the person
/// can try a different one.
#[derive(Clone, Debug)]
pub struct CaptureDialog {
    pub dialog: gtk::Dialog,
    pub clear_button: Option<gtk::Button>,
    message: gtk::Label,
    finished: Rc<Cell<bool>>,
}

impl CaptureDialog {
    /// Opens the dialog. on_result returns whether it accepted the choice; if not, it
    /// should already have called show_refusal.
    pub fn present(
        parent: &gtk::Window,
        language: Language,
        can_clear: bool,
        on_result: impl Fn(&Self, CaptureResult) -> bool + 'static,
    ) -> Self {
        let dialog = gtk::Dialog::builder()
            .title(text(language, Message::CaptureTitle))
            .transient_for(parent)
            .modal(true)
            .use_header_bar(1)
            .default_width(DIALOG_WIDTH)
            .build();

        let clear_button = can_clear
            .then(|| {
                dialog.add_button(
                    text(language, Message::ClearShortcut),
                    gtk::ResponseType::Reject,
                )
            })
            .and_then(|button| button.downcast::<gtk::Button>().ok());

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(CONTENT_SPACING)
            .margin(CONTENT_MARGIN)
            .build();
        let prompt = widgets::label(text(language, Message::CapturePrompt), &[]);
        prompt.set_xalign(0.5);
        let message = widgets::label(text(language, Message::CaptureHint), &[DIM, SMALL]);
        message.set_xalign(0.5);
        message.set_line_wrap(true);
        message.set_justify(gtk::Justification::Center);
        content.add(&prompt);
        content.add(&message);
        dialog.content_area().add(&content);

        let this = Self {
            dialog: dialog.clone(),
            clear_button,
            message,
            finished: Rc::new(Cell::new(false)),
        };
        let on_result = Rc::new(on_result);
        let capture = Rc::new(RefCell::new(ShortcutCapture::default()));

        {
            let this = this.clone();
            let on_result = Rc::clone(&on_result);
            let capture = Rc::clone(&capture);

            dialog.connect_key_press_event(move |_dialog, event| {
                let key = key_code_from_hardware(event.hardware_keycode());
                let key_event = KeyEvent {
                    key,
                    action: KeyAction::Press,
                };

                this.handle_step(
                    capture.borrow_mut().handle(key_event),
                    language,
                    &*on_result,
                );

                glib::Propagation::Stop
            });
        }

        {
            let this = this.clone();
            let on_result = Rc::clone(&on_result);

            dialog.connect_key_release_event(move |_dialog, event| {
                let key = key_code_from_hardware(event.hardware_keycode());

                this.handle_step(
                    capture.borrow_mut().handle(KeyEvent::release(key)),
                    language,
                    &*on_result,
                );

                glib::Propagation::Stop
            });
        }

        {
            let this = this.clone();

            dialog.connect_response(move |dialog, response| {
                if response == gtk::ResponseType::Reject {
                    on_result(&this, CaptureResult::Clear);
                }

                dialog.close();
            });
        }

        dialog.show_all();

        this
    }

    fn handle_step(
        &self,
        step: CaptureStep,
        language: Language,
        on_result: &dyn Fn(&Self, CaptureResult) -> bool,
    ) {
        // Closing happens later, so the keys still being released must not act again.
        if self.finished.get() {
            return;
        }

        match step {
            CaptureStep::Waiting => {}
            CaptureStep::Cancelled => self.finish(),
            CaptureStep::UnknownKey => self.show_refusal(language, Message::UnknownKey),
            CaptureStep::Rejected(ChordError::TooFewModifiers(_)) => {
                self.show_refusal(language, Message::NeedsMoreKeys);
            }
            CaptureStep::Rejected(_) => self.show_refusal(language, Message::UnknownKey),
            CaptureStep::Finished(chord) => {
                if on_result(self, CaptureResult::Set(chord)) {
                    self.finish();
                }
            }
        }
    }

    fn finish(&self) {
        self.finished.set(true);
        self.dialog.close();
    }

    pub fn show_refusal(&self, language: Language, message: Message) {
        self.message.set_text(text(language, message));
    }

    pub fn message_text(&self) -> String {
        self.message.text().to_string()
    }
}
