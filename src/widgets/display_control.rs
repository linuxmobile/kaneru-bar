use crate::windows::DisplayControlWindow;
use gtk4::prelude::*;
use gtk4::Button;
use std::rc::Rc;

pub struct DisplayWidget {
    container: Button,
    window: Rc<DisplayControlWindow>,
}

impl DisplayWidget {
    pub fn new() -> Self {
        let window = DisplayControlWindow::new();
        let popover = window.popover().clone();

        let container = Button::builder()
            .icon_name("video-display-symbolic")
            .css_classes(vec!["display-button"])
            .build();

        popover.set_parent(&container);

        container.connect_clicked(move |button| {
            popover.set_pointing_to(Some(&button.allocation()));
            popover.popup();
        });

        Self { container, window }
    }

    pub fn widget(&self) -> &Button {
        &self.container
    }

    pub fn window(&self) -> &Rc<DisplayControlWindow> {
        &self.window
    }
}
