use std::path::PathBuf;

use pandoc::{OutputKind, PandocOption};
use relm4::{
    Component, ComponentParts, ComponentSender, SimpleComponent,
    actions::{AccelsPlus, RelmAction, RelmActionGroup},
    adw,
    gtk::{self, gio::prelude::FileExt, prelude::ButtonExt},
    main_application,
};

use gtk::prelude::{ApplicationExt, GtkWindowExt, OrientableExt, SettingsExt, WidgetExt};
use gtk::{gio, glib};

use crate::config::{APP_ID, PROFILE};
use crate::modals::{about::AboutDialog, shortcuts::ShortcutsDialog};

pub(super) struct App {
    pub output_file: Option<PathBuf>,
}

#[derive(Debug)]
pub(super) enum AppMsg {
    Quit,
    PickedOutputFile(PathBuf),
    ExportDocument(PathBuf),
    ExportComplete,
    PickInputFile,
    PickOutputFile,
}

relm4::new_action_group!(pub(super) WindowActionGroup, "win");
relm4::new_stateless_action!(PreferencesAction, WindowActionGroup, "preferences");
relm4::new_stateless_action!(pub(super) ShortcutsAction, WindowActionGroup, "show-help-overlay");
relm4::new_stateless_action!(AboutAction, WindowActionGroup, "about");
relm4::new_stateless_action!(QuitAction, WindowActionGroup, "quit");

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = ();
    type Input = AppMsg;
    type Output = ();
    type Widgets = AppWidgets;

    menu! {
        primary_menu: {
            section! {
                "_Preferences" => PreferencesAction,
                "_Keyboard" => ShortcutsAction,
                "_About Pamphlets" => AboutAction,
            }
        }
    }

    view! {
        main_window = adw::ApplicationWindow::new(&main_application()) {
            set_visible: true,

            connect_close_request[sender] => move |_| {
                sender.input(AppMsg::Quit);
                glib::Propagation::Stop
            },

            add_css_class?: if PROFILE == "Devel" {
                    Some("devel")
                } else {
                    None
                },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,

                adw::HeaderBar {
                    pack_end = &gtk::MenuButton {
                        set_icon_name: "open-menu-symbolic",
                        set_menu_model: Some(&primary_menu),
                    }
                },

                gtk::Button {
                    set_label: "Pick output file",
                    connect_clicked => AppMsg::PickOutputFile
                },
                gtk::Button {
                    set_label: "Pick input file",
                    connect_clicked => AppMsg::PickInputFile
                },
                gtk::Label{
                    #[watch]
                    set_label: &model.output_file.as_ref().map(|file| file.to_string_lossy().to_string()).unwrap_or_default(),
                }
            }

        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self { output_file: None };
        let widgets = view_output!();

        let app = root.application().unwrap();
        let mut actions = RelmActionGroup::<WindowActionGroup>::new();

        let shortcuts_action = {
            RelmAction::<ShortcutsAction>::new_stateless(move |_| {
                ShortcutsDialog::builder().launch(()).detach();
            })
        };

        let about_action = {
            RelmAction::<AboutAction>::new_stateless(move |_| {
                AboutDialog::builder().launch(()).detach();
            })
        };

        let quit_action = {
            RelmAction::<QuitAction>::new_stateless(move |_| {
                sender.input(AppMsg::Quit);
            })
        };

        // Connect action with hotkeys
        app.set_accelerators_for_action::<QuitAction>(&["<Control>q"]);

        actions.add_action(shortcuts_action);
        actions.add_action(about_action);
        actions.add_action(quit_action);
        actions.register_for_widget(&widgets.main_window);

        widgets.load_window_size();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            AppMsg::Quit => main_application().quit(),
            AppMsg::ExportComplete => {}
            AppMsg::PickInputFile => {
                let sender = sender.clone();

                relm4::spawn_local(async move {
                    let file_filter =
                        ashpd::desktop::file_chooser::FileFilter::new("MD").glob("*.md");
                    let file_request = ashpd::desktop::file_chooser::OpenFileRequest::default()
                        .filter(file_filter)
                        .multiple(false);
                    if let Ok(file_response) = file_request.send().await.unwrap().response() {
                        let file_uri = file_response.uris().first().unwrap();

                        let source_file = gio::File::for_uri(file_uri.as_str());
                        let input_path = source_file.path().unwrap();

                        sender.input(AppMsg::ExportDocument(input_path));
                    }
                });
            }
            AppMsg::PickedOutputFile(output_file) => {
                self.output_file = Some(output_file);
            }
            AppMsg::PickOutputFile => {
                let sender = sender.clone();

                relm4::spawn_local(async move {
                    let file_filter =
                        ashpd::desktop::file_chooser::FileFilter::new("PDF").glob("*.pdf");
                    let file_request = ashpd::desktop::file_chooser::SaveFileRequest::default()
                        .filter(file_filter);
                    if let Ok(file_response) = file_request.send().await.unwrap().response() {
                        let file_uri = file_response.uris().first().unwrap();

                        let source_file = gio::File::for_uri(file_uri.as_str());
                        let output_path = source_file.path().unwrap();

                        sender.input(AppMsg::PickedOutputFile(output_path));
                    }
                });
            }
            AppMsg::ExportDocument(input_path) => {
                let sender = sender.clone();
                let output_path = self.output_file.clone().unwrap();

                relm4::spawn_local(async move {
                    let mut pandoc = pandoc::new();
                    pandoc.add_input(&input_path);
                    pandoc.add_option(PandocOption::PdfEngine(PathBuf::from("tectonic")));
                    pandoc.set_output(OutputKind::File(output_path));
                    pandoc.set_show_cmdline(true);
                    pandoc.execute().unwrap();
                    sender.input(AppMsg::ExportComplete);
                });
            }
        }
    }

    fn shutdown(&mut self, widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        widgets.save_window_size().unwrap();
    }
}

impl AppWidgets {
    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let settings = gio::Settings::new(APP_ID);
        let (width, height) = self.main_window.default_size();

        settings.set_int("window-width", width)?;
        settings.set_int("window-height", height)?;

        settings.set_boolean("is-maximized", self.main_window.is_maximized())?;

        Ok(())
    }

    fn load_window_size(&self) {
        let settings = gio::Settings::new(APP_ID);

        let width = settings.int("window-width");
        let height = settings.int("window-height");
        let is_maximized = settings.boolean("is-maximized");

        self.main_window.set_default_size(width, height);

        if is_maximized {
            self.main_window.maximize();
        }
    }
}
