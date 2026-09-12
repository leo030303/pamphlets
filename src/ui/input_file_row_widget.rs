use std::path::PathBuf;

use relm4::{
    adw::prelude::{ActionRowExt, PreferencesRowExt},
    gtk::prelude::{ButtonExt, WidgetExt},
    prelude::*,
};

use crate::app::AppMsg;

pub struct InputFileWidget {
    pub file_path: PathBuf,
}

#[relm4::factory(pub)]
impl FactoryComponent for InputFileWidget {
    type Init = PathBuf;
    type Input = AppMsg;
    type Output = AppMsg;
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        adw::ActionRow{
            set_title: self.file_path.file_name().and_then(|name| name.to_str()).unwrap_or("Invalid Unicode"),
            add_prefix = &gtk::Image {
                set_pixel_size: 16,
                set_icon_name: Some("list-drag-handle-symbolic"),
                set_css_classes: &["dimmed"]
            },
            add_suffix = &gtk::Button {
                set_icon_name: "edit-delete-symbolic",
                set_valign: gtk::Align::Center,
                connect_clicked[sender, file_path = self.file_path.clone()] => move |_| {sender.output(AppMsg::RemoveInputFile(file_path.clone())).unwrap()}
            }
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { file_path: init }
    }
}
