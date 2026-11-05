mod app;
mod assets;
mod backend;
mod input;
mod model;
mod preferences;
mod theme;

use app::{Quit, VaultApp};
use gpui::{prelude::*, *};

fn main() {
    let demo = std::env::args().any(|arg| arg == "--preview");
    Application::new()
        .with_assets(assets::Assets)
        .run(move |cx| {
            input::bind_keys(cx);
            app::bind_keys(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.set_menus(vec![
                Menu {
                    name: "ZeroApiKey".into(),
                    items: vec![
                        MenuItem::action("Settings…", app::Settings),
                        MenuItem::separator(),
                        MenuItem::action("Lock Vault", app::Lock),
                        MenuItem::separator(),
                        MenuItem::action("Quit ZeroApiKey", Quit),
                    ],
                },
                Menu {
                    name: "File".into(),
                    items: vec![
                        MenuItem::action("New Entry", app::NewEntry),
                        MenuItem::action("New Group", app::NewGroup),
                        MenuItem::action("Refresh", app::Refresh),
                    ],
                },
                Menu {
                    name: "Edit".into(),
                    items: vec![
                        MenuItem::action("Cut", input::Cut),
                        MenuItem::action("Copy", input::Copy),
                        MenuItem::action("Paste", input::Paste),
                        MenuItem::action("Select All", input::SelectAll),
                    ],
                },
            ]);
            let bounds = Bounds::centered(None, size(px(1100.), px(740.)), cx);
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        window_min_size: Some(size(px(900.), px(600.))),
                        titlebar: Some(TitlebarOptions {
                            title: Some("ZeroApiKey".into()),
                            appears_transparent: true,
                            traffic_light_position: Some(point(px(14.), px(14.))),
                        }),
                        ..Default::default()
                    },
                    |window, cx| cx.new(|cx| VaultApp::new(window, cx, demo)),
                )
                .expect("Cannot open ZeroApiKey window");
            window
                .update(cx, |_, window, cx| {
                    window.on_window_should_close(cx, |_, cx| {
                        cx.quit();
                        true
                    });
                })
                .ok();
            cx.activate(true);
        });
}
