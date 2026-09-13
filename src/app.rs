use std::path::PathBuf;

use pandoc::{OutputKind, PandocOption};
use relm4::{
    Component, ComponentParts, ComponentSender, RelmWidgetExt, SimpleComponent,
    actions::{AccelsPlus, RelmAction, RelmActionGroup},
    adw,
    gtk::{
        self, cairo,
        gdk::{self, Texture},
        gio::{Cancellable, prelude::FileExt},
        glib::object::Cast,
        prelude::ButtonExt,
    },
    main_application,
    prelude::FactoryVecDeque,
};

use gtk::prelude::{ApplicationExt, GtkWindowExt, OrientableExt, SettingsExt, WidgetExt};
use gtk::{gio, glib};

use crate::modals::{about::AboutDialog, shortcuts::ShortcutsDialog};
use crate::{
    config::{APP_ID, PROFILE},
    ui::input_file_row_widget::InputFileWidget,
};

pub(super) struct App {
    pub input_files: Vec<PathBuf>,
    pub input_files_widgets: FactoryVecDeque<InputFileWidget>,
    pub output_file: Option<PathBuf>,
    pub pdf_preview: Option<Texture>,
}

#[derive(Debug)]
pub enum AppMsg {
    Quit,
    PickedOutputFile(PathBuf),
    ExportDocument,
    UpdatePdfPreview(PathBuf),
    OpenInputFilePicker,
    AddInputFiles(Vec<PathBuf>),
    PickOutputFile,
    RemoveInputFile(PathBuf),
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

                if model.input_files.is_empty() {
                    adw::StatusPage {
                        set_title: "Pick your files",
                        set_description: Some("Select the markdown files you want to create your document from"),
                        set_icon_name: Some("x-office-document-symbolic"),
                        set_hexpand: true,
                        set_vexpand: true,
                        gtk::Button {
                            set_label: "Pick Files",
                            set_css_classes: &["pill", "suggested-action"],
                            set_halign: gtk::Align::Center,
                            connect_clicked => AppMsg::OpenInputFilePicker,
                        }

                    }
                } else {
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,

                        #[local_ref]
                        input_files_list_box -> gtk::ListBox {
                            set_css_classes: &["boxed-list"],
                            set_margin_all: 10,
                        },
                        gtk::Button {
                            set_label: "Pick Export Path",
                            connect_clicked => AppMsg::PickOutputFile
                        },
                        gtk::Button {
                            set_label: "Export",
                            #[watch]
                            set_visible: model.output_file.is_some(),
                            connect_clicked => AppMsg::ExportDocument
                        },
                        gtk::Label{
                            #[watch]
                            set_label: &model.output_file.as_ref().map(|file| file.to_string_lossy().to_string()).unwrap_or_default(),
                        },
                        gtk::Picture {
                            #[watch]
                            set_paintable: model.pdf_preview.as_ref(),
                        }
                    }
                }
            }

        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let input_files_widgets: FactoryVecDeque<InputFileWidget> = FactoryVecDeque::builder()
            .launch(gtk::ListBox::default())
            .forward(sender.input_sender(), |output| output);
        let model = Self {
            input_files: vec![],
            input_files_widgets,
            output_file: None,
            pdf_preview: None,
        };
        let input_files_list_box = model.input_files_widgets.widget();
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
            AppMsg::UpdatePdfPreview(pdf_path) => {
                let cancellable: Option<&Cancellable> = None;
                let pdf_doc = poppler::Document::from_gfile(
                    &gio::File::for_path(pdf_path.to_str().unwrap()),
                    None,
                    cancellable,
                )
                .unwrap();
                if let Some(page) = pdf_doc.page(0) {
                    self.pdf_preview = render_page_to_texture(&page, 150.0);
                }
            }
            AppMsg::OpenInputFilePicker => {
                let sender = sender.clone();

                relm4::spawn_local(async move {
                    let file_filter =
                        ashpd::desktop::file_chooser::FileFilter::new("MD").glob("*.md");
                    let file_request = ashpd::desktop::file_chooser::OpenFileRequest::default()
                        .filter(file_filter)
                        .multiple(true);
                    if let Ok(file_response) = file_request.send().await.unwrap().response() {
                        let input_files = file_response
                            .uris()
                            .iter()
                            .filter_map(|file_uri| gio::File::for_uri(file_uri.as_str()).path())
                            .collect();

                        sender.input(AppMsg::AddInputFiles(input_files));
                    }
                });
            }
            AppMsg::AddInputFiles(new_files) => {
                self.input_files.extend(new_files.clone());
                for item in new_files {
                    self.input_files_widgets.guard().push_back(item);
                }
            }
            AppMsg::PickedOutputFile(output_file) => {
                self.output_file = Some(output_file);
            }
            AppMsg::RemoveInputFile(file_to_remove) => {
                if let Some(file_index) = self
                    .input_files
                    .iter()
                    .position(|file| **file == file_to_remove)
                {
                    self.input_files.remove(file_index);
                    self.input_files_widgets.guard().remove(file_index);
                }
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
            AppMsg::ExportDocument => {
                let sender = sender.clone();
                let output_path = self.output_file.clone().unwrap();
                let input_paths = self.input_files.clone();

                relm4::spawn_local(async move {
                    let mut pandoc = pandoc::new();
                    for input in input_paths {
                        pandoc.add_input(&input);
                    }
                    pandoc.add_option(PandocOption::PdfEngine(PathBuf::from("tectonic")));
                    pandoc.set_output(OutputKind::File(output_path.clone()));
                    pandoc.set_show_cmdline(true);
                    pandoc.execute().unwrap();
                    sender.input(AppMsg::UpdatePdfPreview(output_path));
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

fn render_page_to_texture(page: &poppler::Page, dpi: f64) -> Option<gdk::Texture> {
    let (w_pt, h_pt) = page.size();
    let scale = dpi / 72.0;
    let width = (w_pt * scale).ceil() as i32;
    let height = (h_pt * scale).ceil() as i32;

    let mut cairo_image_surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    let cairo_context = cairo::Context::new(&cairo_image_surface).ok()?;

    // Paint white for underneath transparency
    cairo_context.set_source_rgb(1.0, 1.0, 1.0);
    cairo_context.paint().ok()?;

    cairo_context.scale(scale, scale);
    page.render(&cairo_context);
    drop(cairo_context); // release the Context's reference otherwise data() call will fail

    cairo_image_surface.flush();
    let stride = cairo_image_surface.stride() as usize;
    let data = cairo_image_surface.data().ok()?;
    let bytes = glib::Bytes::from(&data[..]);

    Some(
        gdk::MemoryTexture::new(
            width,
            height,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &bytes,
            stride,
        )
        .upcast(),
    )
}
