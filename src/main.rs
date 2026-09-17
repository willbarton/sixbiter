// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

mod actions;
mod appearance;
mod bundle;
mod lens;
mod menu;
mod preferences;
#[cfg(test)]
mod test_support;
mod views;

use gpui_kit::component::Root;
use gpui_kit::{AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size};

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,sixbiter=debug")),
        )
        .with_target(false)
        .init();
}

fn main() {
    init_tracing();

    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);

    app.run(move |cx| {
        gpui_kit::init(cx);
        appearance::init_themes(cx);
        lens::store::init(cx);
        preferences::init(cx);
        views::settings::init(cx);
        views::about::init(cx);
        actions::install(cx);
        menu::init(cx);

        // Bring it to the front
        cx.activate(true);

        let window_options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: None,
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1024.0), px(640.0)),
                cx,
            ))),
            ..Default::default()
        };

        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|cx| views::app::AppView::new(window, cx));

                window
                    .observe_window_appearance(|window, cx| {
                        appearance::sync_theme_from_appearance(cx);
                        window.refresh();
                    })
                    .detach();

                cx.new(|cx| {
                    cx.on_release(|_, cx| cx.quit()).detach();
                    Root::new(view, window, cx)
                })
            })
            .expect("Failed to open window");
        })
        .detach();
    });
}
